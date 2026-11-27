//! CLI contract regressions for carve-rs#4934 through #4937.
use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], input: &[u8]) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

fn file(name: &str, bytes: &[u8]) -> String {
    let path = std::env::temp_dir().join(format!("carve-cli-{}-{name}", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    path.to_str().unwrap().to_string()
}

#[test]
fn fix_write_reports_remaining_findings_on_stdout() {
    let path = file("4934.crv", b"[missing][]\n");
    let (status, stdout, stderr) = run(&["fix", "--write", &path], b"");
    assert_eq!(status, 1);
    assert!(
        stdout.contains("warning unresolved-reference-link"),
        "{stdout}"
    );
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(std::fs::read(&path).unwrap(), b"[missing][]\n");
    let (status, stdout, stderr) = run(&["fix", "--write", "--json", &path], b"");
    assert_eq!(status, 1);
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        report["files"][0]["diagnostics"][0]["code"],
        "unresolved-reference-link"
    );
    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn empty_url_policy_uses_the_default_deny_list() {
    let source = b"[safe](https://example.com) [unsafe](javascript:alert(1))\n";
    let (status, stdout, stderr) = run(&["render", "--url-policy", "{}", "-"], source);
    assert_eq!(status, 0, "{stderr}");
    assert!(stdout.contains("href=\"https://example.com\""), "{stdout}");
    assert!(stdout.contains("href=\"\""), "{stdout}");
    assert!(!stderr.contains("bad url policy"), "{stderr}");
}

#[test]
fn ast_expands_includes_after_lossy_byte_decoding() {
    let root = std::env::temp_dir().join(format!("carve-cli-4936-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("part.crv"), b"inside\n").unwrap();
    let source = root.join("source.crv");
    std::fs::write(&source, b"a\xffb\n\n{{ part.crv }}\n").unwrap();
    let (status, stdout, stderr) = run(&["ast", source.to_str().unwrap()], b"");
    assert_eq!(status, 0, "{stderr}");
    let tree: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(tree["stages"][0], "includeExpansion");
    assert!(stdout.contains("inside"), "{stdout}");
    assert!(!stdout.contains("include-unresolved"), "{stdout}");

    // The same byte-source path reports positions in the original bytes.
    std::fs::write(&source, b"a\xffb\n\n{{ missing.crv }}\n").unwrap();
    let (status, stdout, stderr) = run(&["diagnostics", "--json", source.to_str().unwrap()], b"");
    assert_eq!(status, 0, "{stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let finding = &report["files"][0]["diagnostics"][0];
    assert_eq!(finding["code"], "include-not-found");
    assert_eq!(finding["range"]["startOffset"], 5);
}

#[test]
fn html_import_decodes_invalid_utf8_as_text() {
    let (status, stdout, stderr) = run(&["import", "--from", "html", "-"], b"<p>a\xffb</p>");
    assert_eq!(status, 0, "{stderr}");
    assert_eq!(stdout, "a\u{fffd}b\n");
    assert!(!stderr.contains("tree in JSON"), "{stderr}");
    let (status, stdout, stderr) = run(&["render", "--from-ast", "-"], b"\xff");
    assert_eq!((status, stdout), (2, String::new()));
    assert!(stderr.contains("tree in JSON is not UTF-8"), "{stderr}");
}
