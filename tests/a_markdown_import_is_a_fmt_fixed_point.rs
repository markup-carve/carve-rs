//! `carve migrate --from markdown` writes what `carve fmt` would write back
//! (markup-carve/carve-rs#2972).

use std::io::Write;
use std::process::{Command, Stdio};

fn fmt(source: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["fmt", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn carve fmt");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(source.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("carve fmt");
    assert!(out.status.success(), "carve fmt failed on {source:?}");
    String::from_utf8(out.stdout).expect("utf-8")
}

fn assert_fixed_point(markdown: &str) -> String {
    let value = carve::migrate_markdown(markdown).value;
    assert_eq!(fmt(&value), value, "import of {markdown:?}");
    value
}

#[test]
fn a_lone_reference_definition_has_no_blank_line_above_it() {
    assert_eq!(assert_fixed_point("[foo]: /url\n"), "[foo]: /url\n");
}

#[test]
fn two_lone_reference_definitions_keep_one_blank_line_between() {
    assert_fixed_point("[a]: /x\n[b]: /y\n");
}

#[test]
fn a_reference_definition_after_a_block_keeps_its_blank_line() {
    assert_eq!(
        assert_fixed_point("para\n\n[foo]: /url\n"),
        "para\n\n[foo]: /url\n"
    );
}

#[test]
fn a_raw_span_drops_the_whitespace_ending_a_content_line() {
    let value = assert_fixed_point("<a href=\"foo  \nbar\">\n");
    assert!(!value.contains("foo  "), "{value}");
}

#[test]
fn a_raw_span_drops_a_tab_ending_a_content_line() {
    assert_fixed_point("x <a href=\"foo\t\nbar\"> y\n");
}

#[test]
fn a_raw_span_in_emphasis_drops_it_too() {
    assert_fixed_point("*x <a href=\"foo  \nbar\"> y*\n");
}

#[test]
fn a_raw_span_keeps_whitespace_inside_a_line() {
    let value = assert_fixed_point("x <a href=\"foo  bar\"> y\n");
    assert!(value.contains("foo  bar"), "{value}");
}
