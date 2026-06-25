//! CLI integration: `--no-raw-html` (alias `--safe`) escapes `=html` raw
//! blocks/spans so a host shelling out to the binary on untrusted input can
//! disable raw HTML passthrough without crafting a profile.

use std::io::Write;
use std::process::{Command, Stdio};

/// Run the `carve` binary with `args`, feeding `input` on stdin, return stdout.
fn run(args: &[&str], input: &str) -> String {
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
    assert!(
        out.status.success(),
        "carve exited non-zero: {:?}",
        out.status
    );
    String::from_utf8(out.stdout).expect("utf8 stdout")
}

const RAW_BLOCK: &str = "```=html\n<script>alert(1)</script>\n```";
const RAW_SPAN: &str = "`<img onerror=alert(1)>`{=html}";

#[test]
fn raw_html_emitted_by_default() {
    let out = run(&[], RAW_BLOCK);
    assert!(
        out.contains("<script>"),
        "default should emit raw HTML: {out}"
    );
}

#[test]
fn no_raw_html_escapes_raw_block() {
    let out = run(&["--no-raw-html"], RAW_BLOCK);
    assert!(
        !out.contains("<script>"),
        "raw block should be escaped: {out}"
    );
    assert!(
        out.contains("&lt;script&gt;"),
        "expected escaped script: {out}"
    );
}

#[test]
fn safe_alias_escapes_raw_span() {
    let out = run(&["--safe"], RAW_SPAN);
    assert!(!out.contains("<img"), "raw span should be escaped: {out}");
    assert!(out.contains("&lt;img"), "expected escaped img: {out}");
}

#[test]
fn no_raw_html_composes_with_markdown() {
    let out = run(&["--no-raw-html", "--markdown"], RAW_BLOCK);
    assert!(
        !out.contains("<script>"),
        "markdown raw block should be escaped: {out}"
    );
    assert!(
        out.contains("&lt;script&gt;"),
        "expected escaped script: {out}"
    );
}
