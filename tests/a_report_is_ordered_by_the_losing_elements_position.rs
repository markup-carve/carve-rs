//! A DIAGNOSTIC LIST IS ORDERED BY THE LOSING ELEMENT'S DOCUMENT POSITION
//! (docs/html-import.md, "Result and diagnostics"; markup-carve/carve#1586).
//!
//! The page always said the import diagnostic list is ordered and, until that
//! ticket, never said ordered by what - so this importer's list came out in the
//! order its own walk happened to construct the rows in, which is not the order
//! the losses stand in the document. carve-php already answered in document
//! order; this is carve-rs coming to the same rule.
//!
//! THE BASIS IS THE POSITION OF THE LOSING ELEMENT, and the two things it is
//! easy to confuse it with each have a case below. It is NOT the moment the row
//! was constructed: the adapter footnote pass imports the definitions before
//! the body walk starts, so a note's row is built first and belongs last. And
//! it is NOT the traversal order of the shape the importer reads the parent
//! through: a table's cells are read before its `<caption>` because the caption
//! fills a slot on the finished table, and a list's strays are emitted as
//! blocks ahead of the items.

use carve::{html_to_carve, HtmlImportAdapter, HtmlImportOptions};

fn paths(html: &str) -> Vec<String> {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .report
        .diagnostics
        .into_iter()
        .map(|d| d.path.unwrap_or_default())
        .collect()
}

/// The shape markup-carve/carve#1586 was filed on.
#[test]
fn a_table_caption_is_reported_before_a_cell_below_it() {
    assert_eq!(
        paths("<table>\n<caption onclick=\"x()\">C</caption>\n<tr><td onclick=\"y()\">a</td></tr>\n</table>\n"),
        vec![
            "/table[1]/caption[2]".to_owned(),
            "/table[1]/tr[1]/td[1]".to_owned(),
        ],
    );
}

/// A non-`li` child is emitted as blocks AHEAD of the list, so the walk reaches
/// it before any item - including the items written above it.
#[test]
fn a_list_item_is_reported_before_a_stray_written_after_it() {
    assert_eq!(
        paths("<ul>\n<li onclick=\"x()\">a</li>\n<div onclick=\"y()\">stray</div>\n</ul>\n"),
        vec![
            "/ul[1]/li[1]".to_owned(),
            "/ul[1]/div[4]".to_owned(),
            "/ul[1]/div[4]".to_owned(),
        ],
    );
}

/// THE CASE THAT SEPARATES THE BASIS FROM CONSTRUCTION ORDER. The footnote
/// definitions are imported before the body walk starts, so this row is BUILT
/// first and belongs LAST.
#[test]
fn the_body_is_reported_before_a_footnote_definition_imported_first() {
    let html = concat!(
        "<p onclick=\"a()\">Here is a footnote reference,",
        "<a href=\"#fn1\" class=\"footnoteRef\" id=\"fnref1\"><sup>1</sup></a> and another.</p>\n",
        "<div class=\"footnotes\">\n<hr />\n<ol>\n",
        "<li id=\"fn1\"><p onclick=\"b()\">Here is the footnote.",
        "<a href=\"#fnref1\">&#8617;</a></p></li>\n</ol>\n</div>",
    );
    let report = html_to_carve(
        html,
        &HtmlImportOptions {
            adapter: HtmlImportAdapter::Word,
            ..Default::default()
        },
    )
    .expect("import")
    .report;
    assert_eq!(
        report
            .diagnostics
            .into_iter()
            .map(|d| d.path.unwrap_or_default())
            .collect::<Vec<_>>(),
        vec!["/p[1]".to_owned(), "footnote[1]/p[1]".to_owned()],
    );
}

/// A figure lifts its caption out and builds the target first. This engine
/// already answered in document order here - pinned so it stays that way, and
/// because carve-js did not (markup-carve/carve-js#1358).
#[test]
fn a_figcaption_is_reported_before_the_target_written_after_it() {
    assert_eq!(
        paths("<figure>\n<figcaption onclick=\"x()\">C</figcaption>\n<blockquote onclick=\"y()\"><p>q</p></blockquote>\n</figure>\n"),
        vec![
            "/figure[1]/figcaption[2]".to_owned(),
            "/figure[1]/blockquote[4]".to_owned(),
        ],
    );
}

/// Two losses on ONE element share a position, so the tie keeps the order the
/// rows were built in - which for one element's attributes is the order it
/// spells them.
#[test]
fn two_losses_on_one_element_keep_the_order_the_element_spells_them() {
    let messages: Vec<String> = html_to_carve(
        r#"<p onclick="x()" onmouseover="y()">a</p>"#,
        &HtmlImportOptions::default(),
    )
    .expect("import")
    .report
    .diagnostics
    .into_iter()
    .map(|d| d.message)
    .collect();
    assert_eq!(
        messages,
        vec![
            "Dropped event-handler attribute onclick on <p>".to_owned(),
            "Dropped event-handler attribute onmouseover on <p>".to_owned(),
        ],
    );
}
