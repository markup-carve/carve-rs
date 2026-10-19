use carve::{migrate_markdown, HtmlImportSeverity, MigrationConfidence, MigrationFidelity};
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

fn rows(source: &str, expected: &str, codes: &[(&str, &str)]) {
    let result = migrate_markdown(source);
    let written = if source.ends_with('\n') {
        expected.to_owned()
    } else {
        format!("{expected}\n")
    };
    assert_eq!(result.value, written, "{source}");
    let actual: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .map(|row| (row.code.as_str(), row.fidelity, row.path.as_deref()))
        .collect();
    assert_eq!(
        actual,
        codes
            .iter()
            .map(|(code, fidelity)| (
                *code,
                if *fidelity == "preserved" {
                    MigrationFidelity::Preserved
                } else {
                    MigrationFidelity::Normalized
                },
                Some("line:1")
            ))
            .collect::<Vec<_>>(),
        "{source}"
    );
    assert!(result.report.diagnostics.iter().all(|row| {
        row.severity == HtmlImportSeverity::Info && row.confidence == MigrationConfidence::Exact
    }));
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args([
            "migrate",
            "--from",
            "markdown",
            "--report",
            "-",
            "--check-loss",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
    let report: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(report["diagnostics"].as_array().unwrap().len(), codes.len());
}

#[test]
fn unicode_destinations_keep_authored_bytes_and_exact_assessment() {
    rows(
        "[a](/ü.png) and ![b](/ü.png) and [c](/%C3%BC)\n",
        "[a](/ü.png) and ![b](/ü.png) and [c](/%C3%BC)\n",
        &[
            ("markdown-paragraph", "preserved"),
            ("markdown-link", "preserved"),
            ("markdown-image", "preserved"),
            ("markdown-link", "preserved"),
        ],
    );
    rows(
        "![x](/ü.png)\n",
        "![x](/ü.png)\n",
        &[
            ("markdown-paragraph", "preserved"),
            ("markdown-image", "preserved"),
        ],
    );
    for (source, expected) in [
        (
            "[b](https://bücher.de/ö?q=ä#ü)",
            "[b](https://bücher.de/ö?q=ä#ü)",
        ),
        ("[x](/f&ouml;)", "[x](/fö)"),
        ("[r][id]\n\n[id]: /ü.png\n", "[r][id]\n\n[id]: /ü.png\n"),
        ("[x](/ö\u{00A0}ü)", "[x](/ö%C2%A0ü)"),
        ("[x](/ö\u{202F}ü)", "[x](/ö%E2%80%AFü)"),
        ("[x](/ö\u{0085}ü)", "[x](/ö%C2%85ü)"),
        ("[x](/ö&#127;ü)", "[x](/ö%7Fü)"),
        ("[x](/ö\u{FEFF}ü)", "[x](/ö\u{FEFF}ü)"),
        ("[x](/ö\u{200B}ü)", "[x](/ö\u{200B}ü)"),
        ("[a](/x[y])", "[a](/x[y])"),
    ] {
        assert_eq!(
            migrate_markdown(source).value.trim_end_matches('\n'),
            expected.trim_end_matches('\n'),
            "{source}"
        );
    }
    for source in ["[x](/f&ouml;)", "[x](/ö&#127;ü)"] {
        let result = migrate_markdown(source);
        assert_eq!(result.report.diagnostics.len(), 1);
        assert_eq!(result.report.diagnostics[0].code, "fidelity-unverified");
    }
}

#[test]
fn a_bracketed_pointy_autolink_keeps_its_angle_form_and_exact_report() {
    let source = "<https://e.com/a[b]>\n";
    let result = migrate_markdown(source);
    assert_eq!(result.value, source);

    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args([
            "migrate",
            "--from",
            "markdown",
            "--report",
            "-",
            "--check-loss",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn carve");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), source);

    let report: Value = serde_json::from_slice(&output.stderr).unwrap();
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 2, "{report}");
    for (diagnostic, (code, fidelity)) in diagnostics.iter().zip([
        ("markdown-paragraph", "preserved"),
        ("markdown-autolink", "normalized"),
    ]) {
        assert_eq!(diagnostic["code"], code);
        assert_eq!(diagnostic["severity"], "info");
        assert_eq!(diagnostic["fidelity"], fidelity);
        assert_eq!(diagnostic["confidence"], "exact");
        assert_eq!(diagnostic["path"], "line:1");
    }
    assert_eq!(report["sourceFormat"], json!("markdown"));
}
