#[test]
fn a_heading_after_a_separator_below_a_nested_term_is_outside_the_list() {
    // A term is one line (carve#4861): the separator ends both lists.
    let source = "- * :: t\n: a\n  # H\n  tail\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t</dt>\n        </dl>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>: a</p>\n<section id=\"H\">\n  <h1>H</h1>\n  <p>tail</p>\n</section>"
    );
}

#[test]
fn a_shifted_heading_after_the_separator_is_outside_the_list() {
    let source = "- - :: t\n: a\n   # H\n   tail\n";
    assert!(carve::to_html(source).contains("</ul>\n<p>: a</p>\n<section id=\"H\">"));
}

#[test]
fn another_block_opener_after_the_separator_is_outside_the_list() {
    let source = "- * :: t\n: a\n  ---\n  tail\n";
    assert!(carve::to_html(source).ends_with("</ul>\n<p>: a</p>\n<hr>\n<p>tail</p>"));
}

#[test]
fn a_heading_below_every_item_column_is_outside_the_list() {
    let source = "- * :: t\n: a\n # H\n tail\n";
    assert!(carve::to_html(source).contains("</ul>\n<p>: a</p>\n<section id=\"H\">\n  <h1>H</h1>"));
}
