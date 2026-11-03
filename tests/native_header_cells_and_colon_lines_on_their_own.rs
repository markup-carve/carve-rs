//! carve#4974 (ruling A): the canonical writer writes native `|=` header cells.
//! carve#4863: a colon line is read on its own. Expected output: the tickets'
//! Expected sections and the spec model at carve 0aa7eb40.

use carve::{parse, render_carve, to_html};

fn fmt(source: &str) -> String {
    render_carve(&parse(source)).expect("write")
}

#[test]
fn an_aligned_gfm_header_is_written_natively() {
    assert_eq!(
        fmt("| a | b |\n|:--|--:|\n| 1 | 2 |\n"),
        "|=< a |=> b |\n| 1 | 2 |\n"
    );
}

#[test]
fn an_empty_left_aligned_header_keeps_its_marker() {
    let source = "| | b |\n|:--|---|\n| 1 | 2 |\n";
    let written = fmt(source);
    assert_eq!(written, "|=< |= b |\n| 1 | 2 |\n");
    assert_eq!(to_html(&written), to_html(source));
}

#[test]
fn an_orphan_or_aligned_placeholder_keeps_the_separator() {
    for source in [
        "| ^ | a |\n|---|---|\n| 1 | 2 |\n",
        "| < | a |\n|---|---|\n| 1 | 2 |\n",
        "|> Wide|<|~ Last|\n|:--|:--|:--|\n|a|b|c|\n",
    ] {
        assert!(fmt(source).contains("|---"), "{source}");
    }
}

#[test]
fn a_malformed_opener_leaves_no_state() {
    assert_eq!(
        to_html("::: {.x}\nnot a div\n:::\n").trim(),
        "<p>::: {.x}\nnot a div</p>\n<div>\n\n</div>"
    );
    assert_eq!(
        to_html(":::\n:::e\n::::\nq\n:::\n").trim(),
        "<div>\n  <p>:::e</p>\n  <div>\n    <p>q</p>\n    <div>\n\n    </div>\n  </div>\n</div>"
    );
}
