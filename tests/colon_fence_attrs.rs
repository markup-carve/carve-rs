//! STRICT (djot): a `:::` opener carries no inline attributes. The fence
//! line is the colon fence, an optional type word, and an optional quoted
//! title, and nothing else; any trailing `{...}` (or other non-title text)
//! is diagnosed and dropped; the container keeps its body. Attributes attach via a preceding
//! block-attribute line.

#[test]
fn inline_attributes_on_typed_openers_are_dropped() {
    for src in [
        "::: note {.x}\nb\n:::",
        "::: note{.x}\nb\n:::",
        "::: warning {#w foo=bar}\nb\n:::",
        "::: note \"Heads up\" {.x}\nb\n:::",
    ] {
        let html = carve::to_html(src);
        assert!(html.starts_with("<aside"), "{src}: {html}");
        assert!(!html.contains(":::"));
        assert!(!html.contains("admonition-title"));
        assert!(!html.contains("foo="));
        assert!(carve::lint_carve(src)
            .iter()
            .any(|w| w.rule == "fence-title-syntax"));
    }
}

#[test]
fn a_missing_kind_does_not_identify_a_container() {
    for src in ["::: {.x}\nb\n:::", ":::{k=v}\nb"] {
        let html = carve::to_html(src);
        assert!(html.starts_with("<p>"));
        assert!(!html.contains("<div"));
    }
    assert!(carve::to_html("::: box {.x}\nb").starts_with("<div class=\"box\">"));
}

#[test]
fn bare_titles_are_dropped() {
    let html = carve::to_html("::: note foo\nb\n:::");
    assert!(html.starts_with("<aside"));
    assert!(!html.contains(":::"));
    assert!(!html.contains("admonition-title"));
}

#[test]
fn quoted_title_still_renders_with_braces() {
    assert_eq!(
        carve::to_html("::: note \"Use {x}\"\nb\n:::"),
        "<aside class=\"admonition note\" aria-labelledby=\"adm-1\">\n  <p class=\"admonition-title\" id=\"adm-1\">Use {x}</p>\n  <p>b</p>\n</aside>"
    );
}

#[test]
fn attributes_attach_via_a_preceding_block_attribute_line() {
    // The only way to attribute a div / admonition (strict djot).
    assert_eq!(
        carve::to_html("{#a .lead}\n::: note\nb\n:::"),
        "<aside class=\"admonition note lead\" id=\"a\" aria-label=\"Note\">\n  <p>b</p>\n</aside>"
    );
    assert_eq!(
        carve::to_html("{.x #y}\n:::\nb\n:::"),
        "<div class=\"x\" id=\"y\">\n  <p>b</p>\n</div>"
    );
}

#[test]
fn digit_first_type_word_is_a_generic_div() {
    for src in ["::: 123\nb\n:::", "::: 1a\nb"] {
        let html = carve::to_html(src);
        assert!(
            html.starts_with("<div class=\""),
            "{src:?} should be a div: {html}"
        );
        assert!(html.contains("<div"), "{src:?} should be a div: {html}");
    }
}

#[test]
fn underscore_first_type_word_is_a_div() {
    assert_eq!(
        carve::to_html("::: _box\nb\n:::"),
        "<div class=\"_box\">\n  <p>b</p>\n</div>"
    );
}
