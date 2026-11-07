//! A marker-line attached line block ends at its first blank unless it has a closer.
//! A following opener belongs to the deepest host whose content column it reaches.

use carve::{to_html, to_html_with_options, Options};

fn render(source: &str) -> String {
    let html = to_html(source);
    assert_eq!(html, to_html_with_options(source, &Options::default().with_positions(true)));
    html
}

#[test]
fn colon_opener_one_column_short_of_note_body_opens_in_document() {
    let html = render("[^1]: +\n::: |\n\n :::: \n\n[^1]\n");
    assert!(html.starts_with("<div>\n  <p><a id=\"fnref1\""), "{html}");
    assert!(html.contains("<li id=\"fn1\">\n      <div class=\"line-block\">\n\n      </div>"), "{html}");
    assert!(!html.contains("<p>::::</p>"), "{html}");
}

#[test]
fn same_opener_at_note_body_column_stays_in_note() {
    let html = render("[^1]: +\n::: |\n\n  :::: \n\n[^1]\n");
    assert!(html.contains("<li id=\"fn1\">\n      <div class=\"line-block\">\n\n      </div>\n      <div>"), "{html}");
}
