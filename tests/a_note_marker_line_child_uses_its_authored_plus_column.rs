//! An own-line `+` reaches a child opened on a note marker line at the
//! child's authored marker column (carve#4623, carve#4697).

use carve::{to_carve, to_html};

const REFERENCE: &str =
    "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>";
const PREFIX: &str = "<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n";
const SUFFIX: &str = "      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";

fn expected(body: &str) -> String {
    format!("{REFERENCE}\n{PREFIX}{body}\n{SUFFIX}")
}

#[test]
fn a_quote_takes_a_paragraph_at_its_authored_column() {
    let source = "[^f]: > a\n      +\n      b\n\n[^f]\n";
    let body = "      <blockquote>\n        <p>a</p>\n        <p>b</p>\n      </blockquote>";
    assert_eq!(to_html(source).trim(), expected(body));
}

#[test]
fn a_quote_takes_a_list_at_its_authored_column() {
    let source = "[^f]: > a\n      +\n      - x\n\n[^f]\n";
    let body = "      <blockquote>\n        <p>a</p>\n        <ul>\n          <li>x</li>\n        </ul>\n      </blockquote>";
    assert_eq!(to_html(source).trim(), expected(body));
}

#[test]
fn an_item_takes_its_plus_at_its_authored_column() {
    let source = "[^f]: - i\n      +\n      x\n\n[^f]\n";
    let body = "      <ul>\n        <li>i\n          x\n        </li>\n      </ul>";
    assert_eq!(to_html(source).trim(), expected(body));
}

#[test]
fn an_item_refuses_a_plus_at_the_notes_body_column() {
    let source = "[^f]: - i\n  +\n  x\n\n[^f]\n";
    let body = "      <ul>\n        <li>i\n+\nx</li>\n      </ul>";
    assert_eq!(to_html(source).trim(), expected(body));
}

#[test]
fn formatting_keeps_marker_line_children_at_their_authored_columns() {
    for source in [
        "[^f]: - i\n      +\n      x\n\n[^f]\n",
        "[^long]: 1. i\n         +\n         x\n\n[^long]\n",
        "[^f]: - - a\n        # H\n\n[^f]\n",
        "[^f]: - ```js\n    x\n    ```\n\n[^f]\n",
    ] {
        let formatted = to_carve(source);
        assert_eq!(to_html(&formatted), to_html(source), "{formatted}");
        assert_eq!(to_carve(&formatted), formatted, "{source}");
    }
}
