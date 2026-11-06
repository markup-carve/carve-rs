// carve#5318: the closer of an item's open `%%%` span after a column-0 comment
// line is part of the span, not a block opener that ends the item.

#[test]
fn a_span_closer_after_a_column_0_comment_keeps_the_outer_item() {
    let html = carve::to_html("- i\n  - %%%\n    n\n%% c\n  %%%\n  zz\n");
    assert_eq!(
        html,
        "<ul>\n  <li>i\n    <ul>\n      <li></li>\n    </ul>\n    zz\n  </li>\n</ul>"
    );
}
