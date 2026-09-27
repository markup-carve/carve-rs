//! A `{% … %}` comment whose text begins with a line break takes no pad after
//! the opener (markup-carve/carve#2425).
//!
//! The pad would be the last character on that line, so the canonical writer
//! would invent trailing whitespace the document never held. carve-rs's
//! `normalize` strips a line's trailing run only where the next line is blank,
//! which a comment mid-paragraph is not, so the space survived to the output.
//! carve-js writes the same bytes and is the engine the ruling named.

fn fmt(src: &str) -> String {
    carve::to_carve(src)
}

/// Each pair is measured against carve-js on the same input. The second shows
/// that the pad before the CLOSER is unaffected: it is added even when the text
/// already ends with a line break.
#[test]
fn an_inline_comment_opening_with_a_break_takes_no_pad() {
    for (source, expected) in [
        ("a {%\nA b\n %} c\n", "a {%\nA b\n %} c\n"),
        ("a {%\nA b\n%} c\n", "a {%\nA b\n %} c\n"),
        ("a {% x\ny %} c\n", "a {% x\ny %} c\n"),
    ] {
        let written = fmt(source);
        assert_eq!(written, expected, "{source:?}");
        assert_eq!(fmt(&written), written, "not idempotent: {written:?}");
        assert!(
            written.lines().all(|line| !line.ends_with([' ', '\t'])),
            "the writer left trailing whitespace: {written:?}"
        );
    }
}

/// The block-level arm writes the same comment through its own site, so it needs
/// its own reading.
#[test]
fn a_comment_that_owns_its_block_opening_with_a_break_takes_no_pad() {
    for (source, expected) in [
        ("{%\nA b\n %}\n", "{%\nA b\n %}\n"),
        ("{%\nA b\n%}\n", "{%\nA b\n %}\n"),
        ("{% x\ny %}\n", "{% x\ny %}\n"),
    ] {
        let written = fmt(source);
        assert_eq!(written, expected, "{source:?}");
        assert_eq!(fmt(&written), written, "not idempotent: {written:?}");
    }
}

/// An empty delimited comment still needs both pads: `{%%}` is not the same
/// construct, and the two spaces are what keep two touching backtick runs apart.
#[test]
fn an_empty_delimited_comment_keeps_both_pads() {
    assert_eq!(fmt("a {%  %} b\n"), "a {%  %} b\n");
}
