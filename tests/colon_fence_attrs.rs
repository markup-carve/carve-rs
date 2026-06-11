//! A trailing attribute block on a typed colon-fence opener attaches to the
//! admonition / div wrapper (grammar `admonition_open` `[attributes]`),
//! matching carve-php / carve-js. The attribute-only opener (`:::{...}`) is
//! covered separately by the generic-div corpus.

#[test]
fn class_on_typed_opener() {
    assert_eq!(
        carve::to_html("::: note {.x}\nb\n:::"),
        "<aside class=\"admonition note x\">\n  <p>b</p>\n</aside>"
    );
}

#[test]
fn class_abuts_the_type_word() {
    // The [attributes] slot needs no leading space.
    assert_eq!(
        carve::to_html("::: note{.x}\nb\n:::"),
        "<aside class=\"admonition note x\">\n  <p>b</p>\n</aside>"
    );
}

#[test]
fn id_and_key_value_on_typed_opener() {
    assert_eq!(
        carve::to_html("::: warning {#w foo=bar}\nb\n:::"),
        "<aside class=\"admonition warning\" id=\"w\" foo=\"bar\">\n  <p>b</p>\n</aside>"
    );
}

#[test]
fn title_then_attribute_block() {
    assert_eq!(
        carve::to_html("::: note \"Heads up\" {.x}\nb\n:::"),
        "<aside class=\"admonition note x\">\n  <p class=\"admonition-title\">Heads up</p>\n  <p>b</p>\n</aside>"
    );
}

#[test]
fn braces_inside_title_are_not_the_attribute_block() {
    assert_eq!(
        carve::to_html("::: note \"Use {x}\" {.highlight}\nb\n:::"),
        "<aside class=\"admonition note highlight\">\n  <p class=\"admonition-title\">Use {x}</p>\n  <p>b</p>\n</aside>"
    );
}

#[test]
fn attribute_block_on_a_tier2_typed_div() {
    assert_eq!(
        carve::to_html("::: box {.x}\nb\n:::"),
        "<div class=\"box x\">\n  <p>b</p>\n</div>"
    );
}

#[test]
fn leading_block_attribute_line_merges_with_opener() {
    // Leading attrs precede the opener's classes; the opener wins on
    // id/key conflict (§15).
    assert_eq!(
        carve::to_html("{#a .lead}\n::: note {#b .x}\nb\n:::"),
        "<aside class=\"admonition note lead x\" id=\"b\">\n  <p>b</p>\n</aside>"
    );
}
