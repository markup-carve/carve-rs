//! A table cell's and a term's edges are block edges, so their HTML
//! whitespace is layout and the import writes none (carve-rs#2015).

use carve::{html_to_carve, to_carve, HtmlImportOptions};

fn migrated(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

#[test]
fn a_cell_and_a_term_import_without_edge_space() {
    let html =
        "<table><tr><td> x</td><th>Tokyo </th><td> <a href=\"#a\">A</a> </td></tr></table>\n\
                <dl><dt> <a href=\"#c\">T</a></dt><dd>d</dd></dl>";
    let out = migrated(html);
    assert_eq!(out, "| x |= Tokyo | [A](#a) |\n\n:: [T](#c)\n: d\n");
    assert_eq!(to_carve(&out), out, "the import is a fmt fixed point");
}
