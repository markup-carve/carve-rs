#[test]
fn nested_refused_marker_stays_in_inner_item() {
    assert_eq!(
        carve::to_html("- a\n  - b\n  +\n  +"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n        +\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn root_refused_marker_stays_in_item_before_sibling() {
    assert_eq!(
        carve::to_html("- a\n+\n+\n- b"),
        "<ul>\n  <li>a\n    +\n  </li>\n  <li>b</li>\n</ul>"
    );
}
