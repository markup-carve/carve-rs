//! A top-level GitHub alert blockquote imports as an admonition
//! (CARVE-P2-033, corpus-convert 88-100).

use carve::ast::BlockNode;
use carve::{HtmlImportSeverity, ImportFidelity, MigrationConfidence};
use std::io::Write;
use std::process::{Command, Stdio};

fn imported(markdown: &str) -> String {
    carve::markdown_to_carve(markdown)
        .trim_end_matches('\n')
        .to_owned()
}

fn top_level(carve: &str) -> Vec<&'static str> {
    carve::parse(carve)
        .children
        .iter()
        .map(|node| match node {
            BlockNode::BlockQuote(_) => "block_quote",
            BlockNode::Admonition(_) => "admonition",
            BlockNode::List(_) => "list",
            _ => "other",
        })
        .collect()
}

#[test]
fn the_shared_converter_cases_render_as_expected() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus-convert");
    let mut cases: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|entry| {
            let name = entry.unwrap().file_name().into_string().unwrap();
            let number: usize = name.split('-').next()?.parse().ok()?;
            (88..=100).contains(&number).then_some(name)
        })
        .collect();
    cases.sort();
    assert_eq!(cases.len(), 13, "{cases:?}");
    for case in cases {
        let input = std::fs::read_to_string(root.join(&case).join("input.md")).unwrap();
        let expected = std::fs::read_to_string(root.join(&case).join("expected.html")).unwrap();
        let migrated = carve::migrate_markdown(&input);
        assert_eq!(
            carve::to_html(&migrated.value).trim(),
            expected.trim(),
            "{case}"
        );
        assert!(
            migrated
                .report
                .diagnostics
                .iter()
                .all(|row| row.code != "fidelity-unverified"),
            "{case}: {:?}",
            migrated.report.diagnostics
        );
    }
}

#[test]
fn each_label_names_its_admonition() {
    for (label, kind, name) in [
        ("NOTE", "note", "Note"),
        ("TIP", "tip", "Tip"),
        ("IMPORTANT", "info", "Info"),
        ("WARNING", "warning", "Warning"),
        ("CAUTION", "danger", "Danger"),
    ] {
        let carve = imported(&format!("> [!{label}]\n> Back up first."));
        assert_eq!(carve, format!("::: {kind}\nBack up first.\n:::"));
        assert_eq!(
            carve::to_html(&carve).trim(),
            format!(
                "<aside class=\"admonition {kind}\" aria-label=\"{name}\">\n  <p>Back up first.</p>\n</aside>"
            )
        );
    }
}

#[test]
fn the_body_keeps_its_blocks() {
    let carve = imported(
        "> [!WARNING]\n>\n> First paragraph.\n>\n> - one\n> - two\n>\n> ```js\n> const x = 1\n> ```",
    );
    assert_eq!(
        carve,
        "::: warning\nFirst paragraph.\n\n- one\n- two\n\n```js\nconst x = 1\n```\n:::"
    );
}

#[test]
fn a_quote_in_the_body_stays_a_quote() {
    assert_eq!(
        imported("> [!NOTE]\n> outer\n>\n> > [!TIP]\n> > inner"),
        "::: note\nouter\n\n> [!TIP]\n> inner\n:::"
    );
}

#[test]
fn a_lazy_continuation_line_joins_the_body() {
    assert_eq!(imported("> [!NOTE]\nlazy"), "::: note\nlazy\n:::");
}

#[test]
fn a_literal_colon_fence_in_the_body_stays_escaped() {
    let carve = imported("> [!NOTE]\n> ::: tip\n> t\n> :::");
    assert_eq!(carve, "::: note\n\\::: tip\nt\n\\:::\n:::");
    assert_eq!(top_level(&carve), ["admonition"]);
}

#[test]
fn the_import_is_a_writer_fixed_point() {
    let carve = carve::markdown_to_carve("# T\n\n> [!CAUTION]\n> - a\n> - b\n\ntext\n");
    assert_eq!(carve::to_carve(&carve), carve);
}

#[test]
fn other_quotes_stay_quotes() {
    for markdown in [
        "> [!note]\n> x",
        "> [!Note]\n> x",
        "> [!FOO]\n> x",
        "> [!TIP] hi\n> x",
        "> x\n> [!NOTE]",
        "> \\[!NOTE]\n> x",
        "> [!CAUTION]",
        "> [!CAUTION]\n>\n>",
        "> [!TIP]  \n> x",
        "> [!TIP]\\\n> x",
        ">\n> [!NOTE]\n> x",
    ] {
        let carve = imported(markdown);
        assert!(carve.starts_with('>'), "{markdown:?} => {carve}");
        assert_eq!(
            top_level(&carve),
            ["block_quote"],
            "{markdown:?} => {carve}"
        );
    }
}

#[test]
fn a_nested_alert_stays_a_quote() {
    for (markdown, outer) in [
        ("- > [!NOTE]\n  > x", "list"),
        ("> > [!NOTE]\n> > x", "block_quote"),
        ("[^1]\n\n[^1]: > [!NOTE]\n    > x", "other"),
    ] {
        let carve = imported(markdown);
        assert_eq!(top_level(&carve)[0], outer, "{carve}");
        assert!(!carve.contains(":::"), "{carve}");
    }
}

#[test]
fn the_report_records_the_conversion_as_normalized() {
    let migrated = carve::migrate_markdown("# T\n\n> [!IMPORTANT]\n> Read this.\n");
    assert_eq!(migrated.value, "# T\n\n::: info\nRead this.\n:::\n");
    let rows = &migrated.report.diagnostics;
    assert!(rows.iter().all(|row| row.code != "fidelity-unverified"));
    assert!(rows.iter().all(|row| row.code != "markdown-block-quote"));
    let alert = rows
        .iter()
        .find(|row| row.code == "markdown-github-alert")
        .expect("an alert row");
    assert_eq!(alert.severity, HtmlImportSeverity::Info);
    assert_eq!(alert.fidelity, ImportFidelity::Normalized);
    assert_eq!(alert.confidence, MigrationConfidence::Exact);
    assert_eq!(alert.path.as_deref(), Some("line:3"));
    assert!(rows
        .iter()
        .any(|row| row.code == "markdown-paragraph" && row.path.as_deref() == Some("line:4")));
}

#[test]
fn a_quote_that_stays_a_quote_reports_as_one() {
    let rows = carve::migrate_markdown("> [!note]\n> x\n")
        .report
        .diagnostics;
    assert!(rows.iter().all(|row| row.code != "markdown-github-alert"));
    assert!(rows
        .iter()
        .any(|row| row.code == "markdown-block-quote" && row.path.as_deref() == Some("line:1")));
}

#[test]
fn the_cli_writes_the_admonition_and_a_passing_report() {
    let dir = std::env::temp_dir().join(format!("carve-github-alert-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let report = dir.join("report.json");
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["migrate", "--from", "markdown", "--check-loss", "--report"])
        .arg(&report)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"> [!WARNING]\n> Back up first.\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "::: warning\nBack up first.\n:::\n"
    );
    let report = std::fs::read_to_string(report).unwrap();
    assert!(report.contains("\"markdown-github-alert\""), "{report}");
    assert!(!report.contains("fidelity-unverified"), "{report}");
}
