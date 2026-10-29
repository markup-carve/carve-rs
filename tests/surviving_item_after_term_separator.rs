#[test]
fn a_heading_at_the_surviving_item_column_leaves_a_nested_term() {
    let source = "- * :: t\n: a\n  # H\n  tail\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t\n: a</dt>\n        </dl>\n      </li>\n    </ul>\n    <h1 id=\"H\">H</h1>\n    tail\n  </li>\n</ul>"
    );
}

#[test]
fn a_shifted_heading_opens_on_its_own_base() {
    let source = "- - :: t\n: a\n   # H\n   tail\n";
    assert!(
        carve::to_html(source).contains("</dl>\n      </li>\n    </ul>\n    <h1 id=\"H\">H</h1>")
    );
}

#[test]
fn another_block_opener_uses_the_same_surviving_item() {
    let source = "- * :: t\n: a\n  ---\n  tail\n";
    assert!(carve::to_html(source).contains("</dl>\n      </li>\n    </ul>\n    <hr>\n    tail"));
}

#[test]
fn a_heading_below_every_item_column_stays_in_the_term() {
    let source = "- * :: t\n: a\n # H\n tail\n";
    assert!(carve::to_html(source).contains("</ul>\n<section id=\"H\">\n  <h1>H</h1>"));
}
