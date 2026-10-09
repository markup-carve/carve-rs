//! PART 10 §12 / carve#2796. `1)` and `1.` rendered the same bytes, so a
//! document numbering its steps `1)` printed them `1.`.

use carve::{html_to_carve, parse, render_carve, to_html, HtmlImportOptions};

fn fmt(source: &str) -> String {
    render_carve(&parse(source)).expect("writes")
}

#[test]
fn the_list_names_its_delimiter() {
    assert_eq!(
        to_html("1) first\n2) second\n"),
        "<ol data-delim=\")\">\n  <li>first</li>\n  <li>second</li>\n</ol>"
    );
}

#[test]
fn the_default_delimiter_carries_nothing() {
    assert!(!to_html("1. first\n2. second\n").contains("data-delim"));
    assert!(!to_html("- a\n").contains("data-delim"));
}

#[test]
fn it_trails_type_and_start() {
    assert!(
        to_html("c) gamma\nd) delta\n").contains("<ol type=\"a\" start=\"3\" data-delim=\")\">")
    );
}

#[test]
fn it_leads_the_authored_attributes() {
    assert!(
        to_html("{k=v .attr}\n1) first\n").contains("<ol data-delim=\")\" k=\"v\" class=\"attr\">")
    );
}

#[test]
fn a_nested_list_derives_its_own() {
    assert_eq!(
        to_html("1. outer\n\n   1) inner\n"),
        "<ol>\n  <li>outer\n    <ol data-delim=\")\">\n      <li>inner</li>\n    </ol>\n  </li>\n</ol>"
    );
}

#[test]
fn the_delimiter_survives_a_render_and_import_cycle() {
    let source = fmt("1) one\n2) two\n");
    let imported =
        html_to_carve(&to_html(&source), &HtmlImportOptions::default()).expect("imports");
    assert_eq!(fmt(&imported.value), source);
}
