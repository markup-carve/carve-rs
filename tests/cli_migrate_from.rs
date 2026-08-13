//! CLI integration: `carve migrate --from` reaches every importer the crate
//! ships, not only the HTML one the subcommand started with. `djot_to_carve`
//! was library-only, so the sole way to run it was to link the crate.

use std::io::Write;
use std::process::{Command, Stdio};

/// Run `carve` with `args`, feeding `input` on stdin; returns (stdout, stderr,
/// success).
fn run(args: &[&str], input: &str) -> (String, String, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn carve binary");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait carve binary");
    (
        String::from_utf8(out.stdout).expect("utf8 stdout"),
        String::from_utf8(out.stderr).expect("utf8 stderr"),
        out.status.success(),
    )
}

#[test]
fn migrates_djot() {
    let (out, err, ok) = run(&["migrate", "--from", "djot"], "*bold* and _em_\n");
    assert!(ok, "djot migration should succeed: {err}");
    assert!(out.contains("*bold* and /em/"), "unexpected output: {out}");
}

#[test]
fn migrates_markdown_under_both_names() {
    for name in ["markdown", "md"] {
        let (out, err, ok) = run(&["migrate", "--from", name], "**bold** and _em_\n");
        assert!(ok, "--from {name} should succeed: {err}");
        assert!(out.contains("*bold* and /em/"), "--from {name}: {out}");
    }
}

#[test]
fn migrates_html() {
    let (out, _, ok) = run(&["migrate", "--from", "html"], "<p><b>bold</b></p>");
    assert!(ok, "html migration should succeed");
    assert!(out.contains("*bold*"), "unexpected output: {out}");
}

#[test]
fn rejects_an_unknown_source_format() {
    let (_, err, ok) = run(&["migrate", "--from", "rst"], "x");
    assert!(!ok, "an unknown format should fail");
    assert!(
        err.contains("unknown source format rst"),
        "unexpected stderr: {err}"
    );
}

#[test]
fn names_every_source_format_when_from_is_missing() {
    let (_, err, ok) = run(&["migrate"], "x");
    assert!(!ok, "a missing --from should fail");
    assert!(
        err.contains("html, markdown or djot"),
        "unexpected stderr: {err}"
    );
}

/// The loss report is the HTML importer's alone: Markdown and Djot each parse
/// their source whole and have nothing to report as dropped, so a migration
/// from either ignores those options rather than failing on them.
#[test]
fn ignores_the_html_only_options_for_other_formats() {
    let (out, _, ok) = run(
        &["migrate", "--from", "djot", "--check-loss", "--report", "-"],
        "*bold*\n",
    );
    assert!(ok, "html-only options should not fail a djot migration");
    assert_eq!(out, "*bold*\n");
}
