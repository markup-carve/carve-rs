//! C2-T command dispatch and error categories through the public binary.
use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], input: &str) -> (i32, String, String) {
    let binary = std::env::var("CARVE_CLI_TEST_BIN")
        .unwrap_or_else(|_| env!("CARGO_BIN_EXE_carve").to_string());
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn commands_use_the_same_file_and_error_contract() {
    assert_eq!(run(&["fmt", "-"], "a\n"), (0, "a\n".into(), String::new()));
    assert_eq!(
        run(&["import", "--from", "html", "-"], "<p>a</p>"),
        (0, "a\n".into(), String::new())
    );
    let (status, stdout, stderr) = run(&["migrate-ast", "-"], "not json\n");
    assert_eq!((status, stdout), (2, String::new()));
    assert!(stderr.starts_with("carve[ast-json-invalid]:"));
    let (status, stdout, stderr) = run(&["render", "--bogus", "-"], "a\n");
    assert_eq!((status, stdout), (2, String::new()));
    assert!(stderr.starts_with("carve[usage]:"));
}

/// C2-T-034: a writer loss of C2-W-134 fails the file and writes no source;
/// `--strict-losses` fails it with `render-loss`. An allowed code is no loss
/// (C2-T-025, carve#6746).
#[test]
fn import_fails_the_file_on_a_writer_loss() {
    let html = "<table><tr><td valign=\"baseline\">a</td></tr></table>";
    let line =
        "field-unspellable Carve source cannot spell field valign of a table_cell node here\n";
    let (status, stdout, stderr) = run(&["import", "--from", "html", "-"], html);
    assert_eq!((status, stdout.as_str()), (1, ""));
    assert_eq!(
        stderr,
        format!("carve[writer-refused]: <stdin>: the import lost content\n<stdin>: warning {line}")
    );
    assert_eq!(
        run(
            &[
                "import",
                "--from",
                "html",
                "--allow-loss",
                "field-unspellable",
                "-"
            ],
            html
        ),
        (0, "| a |\n".into(), String::new())
    );
    let (status, stdout, stderr) = run(&["import", "--from", "html", "--strict-losses", "-"], html);
    assert_eq!((status, stdout), (1, String::new()));
    assert_eq!(
        stderr,
        format!("carve[render-loss]: <stdin>: the import lost content\n<stdin>: error {line}")
    );
    let root =
        std::env::temp_dir().join(format!("carve-import-writer-loss-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let lost = root.join("lost.html");
    let kept = root.join("kept.html");
    std::fs::write(&lost, html).unwrap();
    std::fs::write(&kept, "<p>ok</p>").unwrap();
    let (status, stdout, _) = run(
        &[
            "import",
            "--from",
            "html",
            lost.to_str().unwrap(),
            kept.to_str().unwrap(),
        ],
        "",
    );
    assert_eq!((status, stdout), (1, "ok\n".into()));
}

/// carve#6746: CC-2107, CC-2108 and the document `lang` of C2-M-004 fail the
/// file without `--allow-loss` (carve#6737) and write the source with it, also
/// under `--strict-losses` (C2-T-026).
#[test]
fn import_writes_the_source_of_an_allowed_writer_loss() {
    let cases = [
        (
            r#"<table><tr><td>a</td><td rowspan="2">r</td></tr><tr><td colspan="2">w</td></tr></table>"#,
            "| a | r |\n| w | < |\n",
        ),
        (
            r#"<table><thead><tr><th>a</th><th rowspan="2">r</th></tr><tr><th colspan="2">w</th></tr></thead></table>"#,
            "|= a |= r |\n|= w |= < |\n",
        ),
        (
            r#"<table><tr><td>a</td><td rowspan="3">r</td></tr><tr><td colspan="2">w</td></tr><tr><td>b</td></tr></table>"#,
            "| a | r |\n| w | < |\n| b | ^ |\n",
        ),
        (
            r#"<table><tr><td>a</td><td>b</td><td rowspan="2">r</td></tr><tr><td>c</td><td colspan="2">w</td></tr></table>"#,
            "| a | b | r |\n| c | w | < |\n",
        ),
        (r#"<html lang="en"><body><p>a</p></body></html>"#, "a\n"),
    ];
    for (html, source) in cases {
        let (status, stdout, stderr) = run(&["import", "--from", "html", "-"], html);
        assert_eq!((status, stdout.as_str()), (1, ""), "{html}");
        assert!(stderr.starts_with("carve[writer-refused]: <stdin>: the import lost content\n"));
        for strict in [false, true] {
            let mut args = vec![
                "import",
                "--from",
                "html",
                "--allow-loss",
                "field-unspellable",
            ];
            if strict {
                args.push("--strict-losses");
            }
            args.push("-");
            assert_eq!(
                run(&args, html),
                (0, source.into(), String::new()),
                "{html}"
            );
        }
    }
}

#[test]
fn region_command_restores_inclusive_ranges_and_code_markers() {
    assert_eq!(
        run(
            &["region", "demo", "-"],
            "%% #region demo\nline\n%% #endregion\n"
        ),
        (0, "{\"end\":2,\"start\":2}\n".into(), String::new())
    );
    assert_eq!(
        run(
            &["region", "--code", "demo", "-"],
            "// #region demo\ncode\n// #endregion\n"
        ),
        (0, "{\"end\":2,\"start\":2}\n".into(), String::new())
    );
    let (status, _, stderr) = run(&["region", "missing", "-"], "plain\n");
    assert_eq!(status, 1);
    assert!(stderr.contains("include-region-invalid"));
}

#[test]
fn directory_import_converts_links_assets_and_reports_without_overwrite() {
    let root = std::env::temp_dir().join(format!(
        "carve-import-directory-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let source = root.join("source");
    let target = root.join("target");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.md"), "Hello\n\n[Next](b.md)\n").unwrap();
    std::fs::write(source.join("b.md"), "# Next\n").unwrap();
    std::fs::write(source.join("image.png"), b"asset").unwrap();
    let args = [
        "import",
        "--from",
        "gfm",
        "--directory",
        source.to_str().unwrap(),
        "--output",
        target.to_str().unwrap(),
    ];
    let (status, stdout, stderr) = run(&args, "");
    assert_eq!((status, stderr.as_str()), (0, ""));
    assert!(stdout.contains("2 file(s) converted; 1 asset(s) copied"));
    assert_eq!(
        std::fs::read_to_string(target.join("a.crv")).unwrap(),
        "Hello\n\n[Next](b.crv)\n"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("b.crv")).unwrap(),
        "= Next\n"
    );
    assert_eq!(std::fs::read(target.join("image.png")).unwrap(), b"asset");
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(target.join("migration-report.json")).unwrap())
            .unwrap();
    assert_eq!(report["files"][0]["links"][0]["status"], "rewritten");
    let (again, _, _) = run(&args, "");
    assert_eq!(again, 2);
    let mut ignored = args.to_vec();
    ignored.extend(["--allow-loss", "bogus"]);
    let (status, _, stderr) = run(&ignored, "");
    assert_eq!(status, 2);
    assert!(stderr.contains("not supported for directory import"));
}

#[test]
fn directory_import_rejects_source_containment_and_reports_missing_links() {
    let root = std::env::temp_dir().join(format!(
        "carve-import-directory-safety-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let source = root.join("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.md"), "[Missing](absent.md)\n").unwrap();
    let nested = source.join("nested");
    let args = [
        "import",
        "--from",
        "gfm",
        "--directory",
        source.to_str().unwrap(),
        "--output",
        nested.to_str().unwrap(),
    ];
    assert_eq!(run(&args, "").0, 2);
    assert!(!nested.exists());
    let output = root.join("output");
    let args = [
        "import",
        "--from",
        "gfm",
        "--directory",
        source.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
    ];
    assert_eq!(run(&args, "").0, 0);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("migration-report.json")).unwrap())
            .unwrap();
    assert_eq!(report["files"][0]["links"][0]["status"], "unresolved");
    assert_eq!(report["review"], serde_json::json!(["a.md"]));
}

#[test]
fn directory_import_reports_invalid_utf8_as_loss() {
    let root = std::env::temp_dir().join(format!(
        "carve-import-invalid-utf8-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let source = root.join("source");
    let target = root.join("target");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.md"), b"a\xffb\n").unwrap();
    let (status, _, _) = run(
        &[
            "import",
            "--from",
            "gfm",
            "--directory",
            source.to_str().unwrap(),
            "--output",
            target.to_str().unwrap(),
            "--strict-losses",
        ],
        "",
    );
    assert_eq!(status, 1);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(target.join("migration-report.json")).unwrap())
            .unwrap();
    assert_eq!(report["review"], serde_json::json!(["a.md"]));
    assert!(
        report["files"][0]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["code"] == "invalid-utf8-replaced" && row["severity"] == "warning")
    );
}

#[test]
fn directory_import_helper_returns_authored_destination_spelling() {
    let (imported, destinations) =
        carve::try_migrate_markdown_for_directory_with_destinations("[x](a\\(b\\).md)\n").unwrap();
    assert!(imported.source.contains("a(b).md"));
    assert_eq!(
        destinations.get("a(b).md").map(String::as_str),
        Some("a\\(b\\).md")
    );
    let (_, referenced) = carve::try_migrate_markdown_for_directory_with_destinations(
        "[x][ref]\n\n[ref]: a\\(b\\).md\n",
    )
    .unwrap();
    assert_eq!(
        referenced.get("a(b).md").map(String::as_str),
        Some("a\\(b\\).md")
    );
    let (_, fenced) = carve::try_migrate_markdown_for_directory_with_destinations(
        "```text\n[x](a\\(b\\).md)\n```\n\n[x](a(b).md)\n",
    )
    .unwrap();
    assert_eq!(fenced.get("a(b).md").map(String::as_str), Some("a(b).md"));
    let (_, entity) =
        carve::try_migrate_markdown_for_directory_with_destinations("[x](a&amp;b.md)\n").unwrap();
    assert_eq!(entity.get("a&b.md").map(String::as_str), Some("a&amp;b.md"));
    let (_, multiline) =
        carve::try_migrate_markdown_for_directory_with_destinations("[x](\n a\\(b\\).md\n)\n")
            .unwrap();
    assert_eq!(
        multiline.get("a(b).md").map(String::as_str),
        Some("a\\(b\\).md")
    );
}

#[test]
fn ast_and_reports_are_json_values() {
    let (status, stdout, stderr) = run(&["ast", "--no-positions", "--no-includes", "-"], "a\n");
    assert_eq!(status, 0);
    assert_eq!(stderr, "");
    let tree: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(tree["astVersion"], "2.0");
    assert_eq!(tree["children"][0]["children"][0]["value"], "a");
    assert!(tree["children"][0].get("pos").is_none());

    let (status, stdout, stderr) = run(&["ast", "--envelope", "--no-positions", "-"], "a\n");
    assert_eq!((status, stderr.as_str()), (0, ""));
    let envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(envelope["document"]["children"][0].get("pos").is_none());

    let (status, stdout, stderr) = run(&["diagnostics", "--json", "--no-includes", "-"], "a\n");
    assert_eq!(status, 0);
    assert_eq!(stderr, "");
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["files"][0]["file"], "<stdin>");
    assert_eq!(report["files"][0]["diagnostics"], serde_json::json!([]));
}

#[test]
fn render_from_ast_exports_the_supplied_tree() {
    let (status, tree, stderr) = run(&["ast", "--no-positions", "-"], "a\n");
    assert_eq!((status, stderr.as_str()), (0, ""));
    let (status, output, stderr) = run(&["render", "--from-ast", "--to", "plain", "-"], &tree);
    assert_eq!((status, output, stderr), (0, "a\n".into(), String::new()));
}

#[test]
fn batch_flag_on_another_command_is_a_usage_error() {
    let (status, stdout, stderr) = run(&["fmt", "--batch", "-"], "a\n");
    assert_eq!((status, stdout), (2, String::new()));
    assert!(stderr.starts_with("carve[usage]:"));
}
