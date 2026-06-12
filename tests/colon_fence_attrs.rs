//! STRICT (djot): a `:::` opener carries no inline attributes. The fence
//! line is the colon fence, an optional type word, and an optional quoted
//! title, and nothing else; any trailing `{...}` (or other non-title text)
//! makes the line an ordinary paragraph. Attributes attach via a PRECEDING
//! block-attribute line.

#[test]
fn inline_attribute_on_typed_opener_is_a_paragraph() {
    for src in [
        "::: note {.x}\nb\n:::",
        "::: note{.x}\nb\n:::",
        "::: warning {#w foo=bar}\nb\n:::",
        "::: note \"Heads up\" {.x}\nb\n:::",
    ] {
        let html = carve::to_html(src);
        assert!(
            html.starts_with("<p>"),
            "{src:?} should be a paragraph: {html}"
        );
        assert!(
            !html.contains("<aside"),
            "{src:?} should not be an admonition: {html}"
        );
    }
}

#[test]
fn inline_attribute_on_generic_div_is_a_paragraph() {
    for src in [
        "::: {.x}\nb\n:::",
        ":::{k=v}\nb\n:::",
        "::: box {.x}\nb\n:::",
    ] {
        let html = carve::to_html(src);
        assert!(
            html.starts_with("<p>"),
            "{src:?} should be a paragraph: {html}"
        );
        assert!(
            !html.contains("<div"),
            "{src:?} should not be a div: {html}"
        );
    }
}

#[test]
fn unquoted_trailing_text_is_a_paragraph() {
    // Only a quoted title may follow the type word.
    let html = carve::to_html("::: note foo\nb\n:::");
    assert!(html.starts_with("<p>"));
    assert!(!html.contains("<aside"));
}

#[test]
fn quoted_title_still_renders_with_braces() {
    assert_eq!(
        carve::to_html("::: note \"Use {x}\"\nb\n:::"),
        "<aside class=\"admonition note\">\n  <p class=\"admonition-title\">Use {x}</p>\n  <p>b</p>\n</aside>"
    );
}

#[test]
fn attributes_attach_via_a_preceding_block_attribute_line() {
    // The only way to attribute a div / admonition (strict djot).
    assert_eq!(
        carve::to_html("{#a .lead}\n::: note\nb\n:::"),
        "<aside class=\"admonition note lead\" id=\"a\">\n  <p>b</p>\n</aside>"
    );
    assert_eq!(
        carve::to_html("{.x #y}\n:::\nb\n:::"),
        "<div class=\"x\" id=\"y\">\n  <p>b</p>\n</div>"
    );
}
