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
        carve::to_html("{#id .foo}\nText\n"),
        "<p id=\"id\" class=\"foo\">Text</p>"
    );
}

#[test]
fn unterminated_attr_block_stays_literal() {
    // No closing `}` -> not an attribute block; falls back to normal parsing.
    let html = carve::to_html("{#id\nText\n");
    assert!(!html.contains("id=\"id\""), "got: {html}");
}

// A `{...}` line that directly trails paragraph content (no blank line) is a
// block-attribute line: it interrupts the paragraph and floats forward (§15),
// rather than back-attaching to the paragraph or folding in as literal text.
#[test]
fn trailing_attr_line_is_dropped_when_nothing_follows() {
    assert_eq!(carve::to_html("Para\n"), "<p>Para</p>");
}

#[test]
fn trailing_attr_line_floats_forward_to_next_block() {
    assert_eq!(
        carve::to_html("Para\n\n{.class}\nNext\n"),
        "<p>Para</p>\n<p class=\"class\">Next</p>"
    );
}

#[test]
fn trailing_same_line_brace_stays_literal() {
    // No abutting host -> literal inline content, not a paragraph attribute.
    assert_eq!(carve::to_html("Para {.x}\n"), "<p>Para {.x}</p>");
}

#[test]
fn heading_trailing_brace_block_is_literal_text() {
    // djot-strict (spec PART 2 headings; matches carve-js #153): a heading
    // line carries no trailing attribute block -- the brace block is ordinary
    // inline content and the id derives from the full literal text.
    let html = carve::to_html("# H {#x}\n");
    assert!(!html.contains("id=\"x\""), "{html}");
    // Heading ids are case-preserving by default (`H` is kept verbatim).
    assert!(html.contains("<section id=\"H-x\">"), "{html}");
}

#[test]
fn heading_takes_attributes_from_a_preceding_line() {
    let html = carve::to_html("{#x .cls}\n# H\n");
    assert!(html.contains("<section id=\"x\">"), "{html}");
    assert!(html.contains("<h1 class=\"cls\">H</h1>"), "{html}");
}

#[test]
fn adjacent_attr_blocks_on_one_line_merge_for_next_block() {
    assert_eq!(
        carve::to_html("{.c #i}\n# H\n"),
        "<section id=\"i\">\n  <h1 class=\"c\">H</h1>\n</section>"
    );
}

#[test]
fn adjacent_attr_block_classes_merge_in_order() {
    assert_eq!(
        carve::to_html("{.a .b}\n# H\n"),
        "<section id=\"H\">\n  <h1 class=\"a b\">H</h1>\n</section>"
    );
}

// A COMPLETE single line that is a valid attr block followed by a NON-attr
// brace (critic markup, empty, etc.) is NOT a standalone attribute line: the
// multi-line joiner must not "rescue" it by stripping the outer braces and
// parsing an interior `}{` as an unquoted value. It stays literal, matching
// carve-js (`{k=v}{+i+}` -> `<p>{k=v}<ins>i</ins></p>`, never a dropped line).
#[test]
fn complete_line_with_trailing_non_attr_brace_stays_literal() {
    assert_eq!(carve::to_html("{k=v}{+i+}\n"), "<p>{k=v}<ins>i</ins></p>");
    assert_eq!(carve::to_html("{k=v}~s~\n"), "<p>{k=v}<s>s</s></p>");
    assert_eq!(
        carve::to_html("{k=v.w}{-d-}\n"),
        "<p>{k=v.w}<del>d</del></p>"
    );
    assert_eq!(carve::to_html("{k=v}{ }\n"), "<p>{k=v}{ }</p>");
    // A genuine two-brace attr chain still floats forward and is dropped.
    assert_eq!(carve::to_html("\n"), "");
    // A genuinely multi-line block (first line does not close) still works.
    assert_eq!(
        carve::to_html("{k=v .foo}\nT\n"),
        "<p k=\"v\" class=\"foo\">T</p>"
    );
}
