use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_separator_at_an_intermediate_column_returns_to_that_item() {
    let source = "- * 1. :: t\n    : a\n   tail\n   tail\n";
    let html = to_html(source);
    assert!(html.contains("</ol>\n        : a\ntail\ntail"), "{html}");
    assert!(!html.contains("</ul>\n    : a"), "{html}");

    let source = "- * 1. :: t\n    : a\n    - sibling\n    tail\n";
    let html = to_html(source);
    assert!(html.contains("</ol>\n        : a\n        <ul>"), "{html}");
}

#[test]
fn the_released_item_paragraph_allows_later_markers() {
    let source = "- * :: t\n  : a\n   :: sibling\n   tail\n";
    let html = to_html(source);
    assert!(html.contains("</ul>\n    : a\n    <dl>"), "{html}");
    assert!(html.contains("<dt>sibling\ntail</dt>"), "{html}");
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        html
    );

    let source = "- * :: t\n  : a\n   - sibling\n   tail\n";
    let html = to_html(source);
    assert!(html.contains("</ul>\n    : a\n    <ul>"), "{html}");
}

#[test]
fn the_open_description_keeps_only_the_lines_it_owns() {
    let source = "- * :: t\n    : a\n :: sibling\n tail\n";
    let html = to_html(source);
    assert!(html.contains("<dd>a\n:: sibling\ntail</dd>"), "{html}");

    let source = "- * :: t\n    : a\n   :: sibling\n   tail\n";
    let html = to_html(source);
    assert!(html.contains("<dd>a</dd>"), "{html}");
    assert!(
        html.contains("</ul>\n    <dl>\n      <dt>sibling"),
        "{html}"
    );
}
