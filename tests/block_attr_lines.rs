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
