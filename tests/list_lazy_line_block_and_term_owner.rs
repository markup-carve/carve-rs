#[test]
fn a_table_row_shaped_line_in_a_line_block_takes_a_lazy_line() {
    let source = "- a\n  ::: |\n  | x |\ny\n";
    let expected = "<ul>\n  <li>a\n    <div class=\"line-block\">\n      <p>| x |<br>\ny</p>\n    </div>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_line_after_a_closed_nested_line_block_leaves_the_outer_item() {
    let source = "- - ::: |\n    x\n    :::\nafter\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p>x</p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>after</p>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_line_after_a_closed_line_block_under_a_nested_paragraph_leaves_the_list() {
    let source = "- - a\n    ::: |\n    x\n    :::\nafter\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>a\n        <div class=\"line-block\">\n          <p>x</p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>after</p>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn an_opener_after_a_lazy_term_line_stays_in_the_marker_line_sub_item() {
    let source = "- * :: t\n   : d\n    > r\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t\n: d</dt>\n        </dl>\n        <blockquote><p>r</p></blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_heading_after_a_lazy_term_line_stays_in_the_ordered_sub_item() {
    let source = "- 1. :: t\n   : d\n     # h\n";
    let expected = "<ul>\n  <li>\n    <ol>\n      <li>\n        <dl>\n          <dt>t\n: d</dt>\n        </dl>\n        <h1 id=\"h\">h</h1>\n      </li>\n    </ol>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}

#[test]
fn a_sibling_after_the_sub_item_opener_stays_in_the_outer_list() {
    let source = "- * :: t\n   : d\n    > r\n- x\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t\n: d</dt>\n        </dl>\n        <blockquote><p>r</p></blockquote>\n      </li>\n    </ul>\n  </li>\n  <li>x</li>\n</ul>";
    assert_eq!(carve::to_html(source).trim_end(), expected);
}
