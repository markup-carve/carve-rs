//! Consecutive and multi-line standalone block-attribute lines.

#[test]
fn consecutive_attr_lines_merge() {
    let src = "{#id}\n{key=val}\n{.foo .bar}\n{key=val2}\n{.baz}\n{#id2}\nOkay";
    assert_eq!(
        carve::to_html(src),
        "<p id=\"id2\" key=\"val2\" class=\"foo bar baz\">Okay</p>"
    );
}

#[test]
fn multiline_attr_block() {
    assert_eq!(
        carve::to_html("{#id\n .foo}\nText"),
        "<p id=\"id\" class=\"foo\">Text</p>"
    );
}

#[test]
fn unterminated_attr_block_stays_literal() {
    // No closing `}` -> not an attribute block; falls back to normal parsing.
    let html = carve::to_html("{#id\nText");
    assert!(!html.contains("id=\"id\""), "got: {html}");
}

// A `{...}` line that directly trails paragraph content (no blank line) is a
// block-attribute line: it interrupts the paragraph and floats forward (§15),
// rather than back-attaching to the paragraph or folding in as literal text.
#[test]
fn trailing_attr_line_is_dropped_when_nothing_follows() {
    assert_eq!(carve::to_html("Para\n{.class}"), "<p>Para</p>");
}

#[test]
fn trailing_attr_line_floats_forward_to_next_block() {
    assert_eq!(
        carve::to_html("Para\n{.class}\n\nNext"),
        "<p>Para</p>\n<p class=\"class\">Next</p>"
    );
}

#[test]
fn trailing_same_line_brace_stays_literal() {
    // No abutting host -> literal inline content, not a paragraph attribute.
    assert_eq!(carve::to_html("Para {.x}"), "<p>Para {.x}</p>");
}

#[test]
fn heading_trailing_brace_block_is_literal_text() {
    // djot-strict (spec PART 2 headings; matches carve-js #153): a heading
    // line carries no trailing attribute block -- the brace block is ordinary
    // inline content and the id derives from the full literal text.
    let html = carve::to_html("# H {#x}");
    assert!(!html.contains("id=\"x\""), "{html}");
    assert!(html.contains("<section id=\"h-x\">"), "{html}");
}

#[test]
fn heading_takes_attributes_from_a_preceding_line() {
    let html = carve::to_html("{#x .cls}\n# H");
    assert!(html.contains("<section id=\"x\">"), "{html}");
    assert!(html.contains("<h1 class=\"cls\">H</h1>"), "{html}");
}
