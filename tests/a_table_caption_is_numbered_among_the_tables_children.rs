//! A TABLE `<caption>` IS NUMBERED AMONG THE TABLE'S CHILD NODES
//! (markup-carve/carve#1560).
//!
//! PART 12 §16 grants exactly three exemptions from counting among all of the
//! parent's child nodes, and closes the list with a MUST NOT: an `<li>` among
//! the list's items, a `<tr>` among the table's rows, a cell among the cells of
//! its row. The importer reads those three parents through a shape of its own
//! and renumbers them, which is what earns the exemption.
//!
//! A `<caption>` earns none. A table has at most one, so "among the captions"
//! can only ever be `[1]` - there is nothing to renumber, and the step this
//! engine printed was not a basis at all but a hard-coded index that never
//! consulted a position.
//!
//! IT AGREED WITH THE RIGHT ANSWER ONLY FOR A TABLE WRITTEN WITH NO WHITESPACE,
//! which is why no fixture caught it: every caption case in the suite spells
//! the table on one line, where the caption really is the first child. Put
//! `<table>` on its own line and the newline is a text node, so the caption is
//! the second child and `caption[1]` named a node the reader does not have.
//!
//! `caption[1]` is also what a reader gets from resolving the path as XPath, so
//! a wrong step here does not read as wrong - it reads as the answer to a
//! different question. That is the reading §16 exists to head off, and it is
//! why this is pinned rather than fixed quietly.

use carve::{html_to_carve, HtmlImportDiagnosticCode, HtmlImportOptions};

fn paths(html: &str, code: HtmlImportDiagnosticCode) -> Vec<Option<String>> {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .report
        .diagnostics
        .into_iter()
        .filter(|d| d.code == code)
        .map(|d| d.path)
        .collect()
}

fn dropped_at(html: &str) -> Vec<Option<String>> {
    paths(html, HtmlImportDiagnosticCode::AttributeDropped)
}

/// The ticket's own input. The newline after `<table>` is the first child, so
/// the caption is the second - the reading carve-php already printed.
#[test]
fn a_pretty_printed_table_counts_the_whitespace_text_node_before_its_caption() {
    assert_eq!(
        dropped_at("<table>\n<caption onclick=\"x()\">C</caption>\n<tr><td>a</td></tr>\n</table>"),
        vec![Some("/table[1]/caption[2]".to_owned())],
    );
}

/// The compact spelling, where the old literal happened to be right. It has to
/// stay right, or the fix would be a second wrong answer.
#[test]
fn a_caption_that_really_is_the_first_child_is_still_caption_1() {
    assert_eq!(
        dropped_at(r#"<table><caption onclick="x()">C</caption><tr><td>a</td></tr></table>"#),
        vec![Some("/table[1]/caption[1]".to_owned())],
    );
}

/// A `<colgroup>` is dropped whole and contributes no step of its own, but it
/// is still a child of the table, so it still moves the caption's index.
#[test]
fn every_child_kind_before_the_caption_counts_not_only_whitespace() {
    assert!(dropped_at(
        r#"<table><colgroup><col></colgroup><caption id="c">C</caption><tr><td>a</td></tr></table>"#
    )
    .contains(&Some("/table[1]/caption[2]".to_owned())));
}

/// The tell that this was a defect rather than a spelling latitude: the
/// second-caption diagnostic went through the child-index helper and the first
/// one through a literal, so one element kind spoke under two bases in a single
/// document. Both are child indices now.
#[test]
fn the_first_caption_is_numbered_on_the_same_basis_as_the_second() {
    let html = "<table>\n<caption onclick=\"x()\">A</caption>\n<caption id=\"b\">B</caption>\n<tr><td>a</td></tr>\n</table>";
    assert!(dropped_at(html).contains(&Some("/table[1]/caption[2]".to_owned())));
    assert_eq!(
        paths(html, HtmlImportDiagnosticCode::TableDegraded),
        vec![Some("/table[1]/caption[4]".to_owned())],
    );
}

/// The path does not turn on WHICH attribute could not be carried. This engine
/// reports one row per attribute, so both rows have to name the same node.
#[test]
fn the_same_path_is_reported_whichever_attribute_is_dropped() {
    let rows = dropped_at(
        "<table>\n<caption id=\"tc\" class=\"k\">C</caption>\n<tr><td>a</td></tr>\n</table>",
    );
    assert!(!rows.is_empty(), "expected a row per dropped attribute");
    assert!(
        rows.iter()
            .all(|p| p.as_deref() == Some("/table[1]/caption[2]")),
        "every row names the caption's child index, got {rows:?}"
    );
}
