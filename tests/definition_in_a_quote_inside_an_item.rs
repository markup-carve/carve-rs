//! A definition inside a block quote is collected, and that does not stop being
//! true one container deeper (carve-rs#588).
//!
//! At top level every engine agrees: `> [r]: /u` empties the quote and the
//! reference resolves. With the quote at a list item's content column
//! (`- a` / `  > [r]: /u`) this engine rendered the definition as quote content
//! instead, because the prefix scan only reads a marker at position 0 - so it
//! disagreed with its own answer one level up.

use carve::to_html;

#[test]
fn a_link_definition_in_a_quote_inside_an_item_resolves() {
    let html = to_html("- a\n  > [r]: /u\n\nsee [t][r]\n");

    assert!(html.contains("href=\"/u\""), "not resolved: {html}");
    assert!(!html.contains("[r]: /u"), "line still rendered: {html}");
}

#[test]
fn the_footnote_form_resolves_too() {
    let html = to_html("- a\n  > [^f]: x\n\nsee[^f]\n");

    assert!(html.contains("doc-endnotes"), "not registered: {html}");
    assert!(!html.contains("[^f]: x"), "line still rendered: {html}");
}

#[test]
fn the_top_level_form_is_unchanged() {
    assert!(to_html("> [r]: /u\n\nsee [t][r]\n").contains("href=\"/u\""));
}

#[test]
fn arbitrary_indentation_is_still_not_a_quote() {
    // The bound: EXACTLY the item's content column counts. A top-level
    // `    > [r]: /u` is indented text - carve-php agrees, carve-js collects it
    // (a separate divergence, deliberately not changed here).
    let html = to_html("[x][r] here.\n\n    > [r]: /u");

    assert!(!html.contains("href=\"/u\""), "should stay text: {html}");
}
