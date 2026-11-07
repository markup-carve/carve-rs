//! An exact closer after a blank keeps a marker-line attached line block open.
//! The closed block retains its verse and consumes the closer.

use carve::{to_html, to_html_with_options, Options};

fn render(source: &str) -> String {
    let html = to_html(source);
    assert_eq!(
        html,
        to_html_with_options(source, &Options::default().with_positions(true))
    );
    html
}

#[test]
fn exact_closer_after_blank_keeps_line_block_open() {
    let html = render("[^1]: +\n::: |\n\n :::\n\n[^1]\n");
    assert_eq!(html, concat!(
        "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n",
        "<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n",
        "  <hr>\n  <ol>\n    <li id=\"fn1\">\n",
        "      <div class=\"line-block\">\n\n      </div>\n",
        "      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n",
        "    </li>\n  </ol>\n</section>"
    ));
}

#[test]
fn verse_between_blank_and_below_column_closer_stays_in_line_block() {
    let html = render("[^1]: +\n::: |\n\na\n :::\n\n[^1]\n");
    assert!(
        html.contains("<div class=\"line-block\">\n        <p>a</p>\n      </div>"),
        "{html}"
    );
    assert!(!html.contains("<p>:::</p>"), "{html}");
}

#[test]
fn verse_spacing_is_preserved_until_the_below_column_closer() {
    let html = render("[^1]: +\n::: |\n\n  b\n ::: \n\n[^1]\n");
    assert!(
        html.contains("<div class=\"line-block\">\n        <p>&nbsp;&nbsp;b</p>\n      </div>"),
        "{html}"
    );
}

#[test]
fn comment_inside_closed_attached_line_block_leaves_empty_verse() {
    let html = render("[^1]: +\n::: |\n\n%% c\n ::: \n\n[^1]\n");
    assert!(
        html.contains("<div class=\"line-block\">\n\n      </div>"),
        "{html}"
    );
    assert!(!html.contains("<p></p>"), "{html}");
}

#[test]
fn definition_shaped_verse_before_closer_stays_in_attached_line_block() {
    let html = render("[^a]: +\n::: |\n\n[^b]: b\n :::\n\n[^a]\n");
    assert!(
        html.contains("<div class=\"line-block\">\n        <p>[^b]: b</p>\n      </div>"),
        "{html}"
    );
    assert!(!html.contains("<li id=\"fn2\">"), "{html}");
}

#[test]
fn tab_indented_verse_maps_columns_without_underflow() {
    let html = render("[^1]: +\n::: |\n\n\ta\n  :::\n\n[^1]\n");
    assert!(
        html.contains("<div class=\"line-block\">\n        <p>&nbsp;&nbsp;a</p>\n      </div>"),
        "{html}"
    );
}
