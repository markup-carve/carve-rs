//! An imported HTML list keeps the tightness its source spelled.
//!
//! Ruled on markup-carve/carve#1210 (spec docs/html-import.md "Lists keep the
//! source's tightness"; corpus-convert 27/28): a bare-text `<li>one</li>` is a
//! tight item, a paragraph-wrapped `<li><p>one</p></li>` a loose one, and
//! import preserves the source's own markup rather than normalizing it.
//! Carve spells tightness per LIST, so a MIXED list resolves the way
//! CommonMark resolves it: one paragraph item loosens the whole list. Only a
//! direct `<p>` votes - a nested list beside bare text is structure, not a
//! paragraph wrapper. Before this every import was loose, so
//! `<ul><li>one</li></ul>` came back as `<ul><li><p>one</p></li></ul>`.

fn round_trip(html: &str) -> String {
    let result = carve::html_to_carve(html, &carve::HtmlImportOptions::default())
        .expect("the fragment imports");
    carve::to_html(&result.value)
}

#[test]
fn bare_text_items_import_tight() {
    assert_eq!(
        round_trip("<ul><li>one</li><li>two</li></ul>"),
        "<ul>\n  <li>one</li>\n  <li>two</li>\n</ul>"
    );
}

#[test]
fn paragraph_wrapped_items_stay_loose() {
    assert_eq!(
        round_trip("<ul><li><p>one</p></li><li><p>two</p></li></ul>"),
        "<ul>\n  <li><p>one</p></li>\n  <li><p>two</p></li>\n</ul>"
    );
}

#[test]
fn one_paragraph_item_loosens_the_whole_list() {
    // The mixed list, corpus-convert 28: the bare item's paragraph-ness is
    // the resolvable half, the wrapped item's paragraph is not - so the list
    // resolves loose and keeps it.
    assert_eq!(
        round_trip("<ul><li>one</li><li><p>two</p></li></ul>"),
        "<ul>\n  <li><p>one</p></li>\n  <li><p>two</p></li>\n</ul>"
    );
}

#[test]
fn a_nested_list_beside_bare_text_does_not_loosen() {
    assert_eq!(
        round_trip("<ul><li>one<ul><li>inner</li></ul></li><li>two</li></ul>"),
        "<ul>\n  <li>one\n    <ul>\n      <li>inner</li>\n    </ul>\n  </li>\n  <li>two</li>\n</ul>"
    );
}

#[test]
fn an_ordered_list_takes_the_same_rule() {
    assert_eq!(
        round_trip("<ol><li>one</li><li>two</li></ol>"),
        "<ol>\n  <li>one</li>\n  <li>two</li>\n</ol>"
    );
}
