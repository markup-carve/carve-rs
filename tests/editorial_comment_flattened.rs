//! An editorial comment is visible content. Markdown wraps it in the span the
//! HTML target uses; Plain and ANSI write it as bare text and report one
//! `editorial-comment-flattened` row per comment (markup-carve/carve#2791).

use carve::{CheckedRenderOptions, RenderTarget};
use std::io::Write;
use std::process::{Command, Stdio};

const SOURCE: &str = "Text {+neu+} und {#Notiz#} hier.\n";
const MESSAGE: &str = "Flattened an editorial comment into the surrounding text";

#[test]
fn markdown_wraps_the_comment_in_a_critic_comment_span() {
    let out = carve::to_markdown(SOURCE);
    assert_eq!(
        out,
        "Text <ins>neu</ins> und <span class=\"critic-comment\">Notiz</span> hier.\n"
    );
}

#[test]
fn markdown_escapes_the_comment_as_one_text_run() {
    let out = carve::to_markdown("a {#x < y & *z* <b> &amp;#} b\n");
    assert_eq!(
        out,
        "a <span class=\"critic-comment\">x < y & \\*z\\* \\<b> \\&amp;</span> b\n"
    );
    // The same characters typed as ordinary text escape the same way.
    let text = carve::to_markdown("x < y & \\*z\\* <b> &amp;\n");
    assert_eq!(text, "x < y & \\*z\\* \\<b> \\&amp;\n");
}

#[test]
fn plain_and_ansi_report_one_row_per_comment_in_document_order() {
    let source = "Text {+neu+} und {#Notiz#} hier {#zwei#}.\n";
    for target in [RenderTarget::Plain, RenderTarget::Ansi] {
        let result = match target {
            RenderTarget::Plain => {
                carve::to_plain_text_with_report(source, CheckedRenderOptions::default())
            }
            _ => carve::to_ansi_with_report(source, CheckedRenderOptions::default()),
        }
        .unwrap();
        assert!(result.value.contains("Notiz"), "{:?}", result.value);
        assert!(result.value.contains("zwei"), "{:?}", result.value);
        assert_eq!(result.total_losses, 2);
        assert_eq!(
            result.totals_by_code.get("editorial-comment-flattened"),
            Some(&2)
        );
        assert!(!result.truncated);
        let columns: Vec<usize> = result
            .losses
            .iter()
            .map(|loss| {
                assert_eq!(loss.code, "editorial-comment-flattened");
                assert_eq!(loss.target, target);
                assert_eq!(loss.node_type.as_str(), "inline");
                assert_eq!(loss.format, None);
                assert_eq!(loss.message, MESSAGE);
                let pos = loss.pos.as_ref().expect("the row carries the node's pos");
                assert_eq!(pos.start_line, 1);
                pos.start_column
            })
            .collect();
        assert_eq!(columns, vec![18, 33], "{target:?}");
    }
}

#[test]
fn plain_output_is_unchanged() {
    assert_eq!(carve::to_plain_text(SOURCE), "Text neu und Notiz hier.\n");
}

#[test]
fn strict_render_refuses_a_flattened_comment() {
    let strict = CheckedRenderOptions {
        strict: true,
        ..CheckedRenderOptions::default()
    };
    let err = carve::to_plain_text_with_report(SOURCE, strict).unwrap_err();
    assert_eq!(err.total_losses, 1);
    assert_eq!(err.losses[0].code, "editorial-comment-flattened");
}

#[test]
fn markdown_html_and_carve_report_no_row() {
    let options = CheckedRenderOptions::default();
    for result in [
        carve::to_markdown_with_report(SOURCE, options).unwrap(),
        carve::to_html_with_report(SOURCE, options).unwrap(),
        carve::to_carve_with_report(SOURCE, options).unwrap(),
    ] {
        assert_eq!(result.total_losses, 0, "{:?}", result.value);
        assert!(result.losses.is_empty());
    }
}

fn cli(args: &[&str]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
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
        .write_all(SOURCE.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn cli_strict_losses_refuses_and_allow_loss_accepts() {
    for target in ["--plain", "--ansi"] {
        let refused = cli(&[target, "--strict-losses"]);
        assert!(!refused.status.success(), "{target}");
        assert!(refused.stdout.is_empty(), "{target}");

        let allowed = cli(&[
            target,
            "--strict-losses",
            "--allow-loss",
            "editorial-comment-flattened",
        ]);
        assert!(allowed.status.success(), "{target}: {:?}", allowed.stderr);
        assert!(String::from_utf8_lossy(&allowed.stdout).contains("Notiz"));
    }
}

#[test]
fn cli_report_carries_the_exact_row() {
    let path = std::env::temp_dir().join(format!(
        "carve-editorial-comment-{}.json",
        std::process::id()
    ));
    let output = cli(&["--plain", "--report-losses", path.to_str().unwrap()]);
    assert!(output.status.success(), "{:?}", output.stderr);
    let report = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(path);
    assert!(report.contains("\"totalLosses\":1"), "{report}");
    assert!(
        report.contains("\"code\":\"editorial-comment-flattened\""),
        "{report}"
    );
    assert!(report.contains("\"target\":\"plain\""), "{report}");
    assert!(report.contains("\"nodeType\":\"inline\""), "{report}");
    assert!(
        report.contains(&format!("\"message\":\"{MESSAGE}\"")),
        "{report}"
    );
    assert!(!report.contains("\"format\""), "{report}");
    assert!(report.contains("\"startColumn\":18"), "{report}");
}
