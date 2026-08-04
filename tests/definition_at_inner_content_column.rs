//! A definition reaches ANY open item's content column, not just the innermost.
//!
//! `- - a` opens two items on one line, so two content columns are live under
//! it: 2 for the outer item and 4 for the inner one. The prepass recorded only
//! the outer, so a definition written at column 4 - the inner item's own
//! content column - looked like text here and registered nothing, while
//! carve-js and carve-php read it as that item's block (carve#655).
//!
//! The failure was not only a wrong shape. `carve fmt` re-indents the
//! below-column form (corpus 183) to column 4, which round-trips inside this
//! engine and LOSES the line in the other two: they consume the definition the
//! formatted document now carries.

use carve::to_html;

#[test]
fn a_definition_at_the_inner_content_column_registers() {
    let html = to_html("- - a\n    [^f]: x\n\nsee[^f]\n");

    assert!(html.contains("doc-endnotes"), "note not registered: {html}");
    assert!(
        !html.contains("[^f]: x"),
        "definition line still rendered: {html}"
    );
}

#[test]
fn a_link_definition_at_the_inner_content_column_registers() {
    let html = to_html("- - a\n    [r]: /u\n\nsee [t][r]\n");

    assert!(
        html.contains("href=\"/u\""),
        "reference not resolved: {html}"
    );
}

#[test]
fn a_definition_at_the_outer_content_column_still_registers() {
    let html = to_html("- - a\n  [^f]: x\n\nsee[^f]\n");

    assert!(html.contains("doc-endnotes"), "note not registered: {html}");
}

#[test]
fn a_definition_between_two_content_columns_stays_text() {
    // Column 3 reaches neither item, so PART 9 §24 C3's fold applies: the line
    // is item paragraph text and defines nothing. All three engines agree.
    let html = to_html("- - a\n   [^f]: x\n\nsee[^f]\n");

    assert!(html.contains("[^f]: x"), "line should stay text: {html}");
    assert!(
        !html.contains("doc-endnotes"),
        "nothing should register: {html}"
    );
}
