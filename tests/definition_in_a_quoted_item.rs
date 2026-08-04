//! Content columns are measured INSIDE a block quote (carve#658).
//!
//! `> - a` puts the item's content column at 2 of the QUOTED content. The
//! definition prepasses fed the raw line to the column tracker, which matches
//! no marker behind a `> `, and `at_content_column` additionally refused to
//! strip whenever anything had been stripped as structural - a blockquote
//! prefix included. So a definition written at the item's column was left with
//! its indent, failed the definition test, and rendered as item text where
//! carve-js and carve-php now register it.

use carve::to_html;

#[test]
fn a_link_definition_at_the_quoted_item_column_resolves() {
    let html = to_html("> - a\n>   [r]: /u\n\nsee [t][r]\n");

    assert!(html.contains("href=\"/u\""), "not resolved: {html}");
    assert!(!html.contains("[r]: /u"), "line still rendered: {html}");
}

#[test]
fn the_footnote_form_resolves_at_the_same_column() {
    // The two definition kinds must answer the same question the same way.
    let html = to_html("> - a\n>   [^f]: x\n\nsee[^f]\n");

    assert!(html.contains("doc-endnotes"), "not registered: {html}");
    assert!(!html.contains("[^f]: x"), "line still rendered: {html}");
}

#[test]
fn a_compact_nested_quoted_item_resolves_too() {
    assert!(to_html("> - - a\n>   [r]: /u\n\nsee [t][r]\n").contains("href=\"/u\""));
}

#[test]
fn one_column_short_it_stays_item_text() {
    let html = to_html("> - a\n>  [r]: /u\n\nsee [t][r]\n");

    assert!(html.contains("[r]: /u"), "should stay text: {html}");
    assert!(!html.contains("href=\"/u\""), "should not resolve: {html}");
}

#[test]
fn a_marker_in_the_structural_prefix_still_disqualifies_the_strip() {
    // `- [r]: /u` has its column consumed by the marker itself; stripping again
    // would eat the item's own content. Unchanged by this fix.
    assert!(to_html("- [r]: /u\n\nsee [t][r]\n").contains("href=\"/u\""));
}

#[test]
fn the_unquoted_shape_is_unchanged() {
    assert!(to_html("- a\n  [r]: /u\n\nsee [t][r]\n").contains("href=\"/u\""));
}

#[test]
fn a_quoted_item_stays_tight_after_the_definition_is_consumed() {
    // The definition renders nothing, so it is not the item's second block and
    // must not loosen the list (§17 L2). Removing the line leaves the quote
    // prefix behind, and that prefix ALONE reads as a blank line inside the
    // item - which loosened it while the fix above was being written. carve-js
    // and carve-php keep it tight.
    assert_eq!(
        to_html("> - a\n>   [^f]: x\n> - b\n\nsee[^f]\n")
            .split("<p>see")
            .next()
            .unwrap(),
        "<blockquote>\n  <ul>\n    <li>a</li>\n    <li>b</li>\n  </ul>\n</blockquote>\n"
    );
}

#[test]
fn an_unquoted_definition_after_a_blank_still_loosens() {
    // The control for that filler: at top level there is no container prefix,
    // so nothing is inserted and §17 L1b still sees the blank separation.
    assert_eq!(
        to_html("- a\n\n  [r]: /u\n  text\n"),
        "<ul>\n  <li><p>a</p>\n    <p>text</p>\n  </li>\n</ul>"
    );
}
