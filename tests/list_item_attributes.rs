//! An attribute block abutting a list marker (no space before `{`) attaches
//! to the `<li>` (grammar `item_attributes`). The corpus `87-list-item-
//! attributes` pins the spec cases; these cover marker variants and the
//! rejection rules that keep `-{...}` from over-firing.

#[test]
fn class_on_star_bullet() {
    assert_eq!(
        carve::to_html("*{.c} star bullet"),
        "<ul>\n  <li class=\"c\">star bullet</li>\n</ul>"
    );
}

#[test]
fn paren_delimited_ordered_marker() {
    assert_eq!(
        carve::to_html("1){#x} paren delim"),
        "<ol>\n  <li id=\"x\">paren delim</li>\n</ol>"
    );
}

#[test]
fn id_and_classes_keep_source_order() {
    assert_eq!(
        carve::to_html("-{#i .a .b} multi"),
        "<ul>\n  <li id=\"i\" class=\"a b\">multi</li>\n</ul>"
    );
}

#[test]
fn empty_block_is_a_bare_item() {
    assert_eq!(carve::to_html("-{} bare"), "<ul>\n  <li>bare</li>\n</ul>");
}

#[test]
fn space_before_brace_is_literal_content() {
    // A space before `{` makes it ordinary content, not an item-attribute.
    assert_eq!(
        carve::to_html("- {.c} text"),
        "<ul>\n  <li>{.c} text</li>\n</ul>"
    );
}

#[test]
fn invalid_block_is_not_a_list() {
    // `{+a+}` is not a valid attribute list, so `-{+a+}` is not a marker;
    // the line is an ordinary paragraph and `+a+` is editorial markup.
    assert_eq!(carve::to_html("-{+a+} text"), "<p>-<ins>a</ins> text</p>");
}

#[test]
fn missing_required_space_is_not_a_list() {
    // The marker's required space must follow the block.
    assert_eq!(carve::to_html("-{.c}text"), "<p>-{.c}text</p>");
}

#[test]
fn attributed_marker_with_no_content_is_not_a_list() {
    // A marker with no same-line content is ordinary text (matches carve-js
    // / carve-php); the attribute block does not rescue an empty item. The
    // inline content is then parsed as prose, so assert only that no list
    // forms (`{#x}` becomes a tag span, etc.).
    for src in ["-{.c} ", "1.{#x} "] {
        let html = carve::to_html(src);
        assert!(
            !html.contains("<li") && !html.contains("<ul") && !html.contains("<ol"),
            "{src:?} should not be a list, got: {html}"
        );
    }
}

#[test]
fn bare_marker_with_no_content_is_not_a_list() {
    // The same rule applies to a plain marker: a bare `- ` is a paragraph.
    // The final trailing space is stripped (CommonMark "final spaces"), so the
    // paragraph renders as `<p>-</p>` rather than keeping the dangling space.
    assert_eq!(carve::to_html("- "), "<p>-</p>");
}

#[test]
fn attributed_bullet_folds_into_a_paragraph() {
    // Symmetric §10: an attributed bullet does NOT interrupt an open paragraph
    // (like a plain bullet); with no preceding blank line it folds in.
    assert_eq!(
        carve::to_html("para\n-{.c} item"),
        "<p>para\n-{.c} item</p>"
    );
}
