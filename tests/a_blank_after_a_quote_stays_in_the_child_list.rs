//! PART 9 §11 N1 and §17 L1: a blank between compatible child items
//! loosens their list, even when a quote preceded its first item.

#[test]
fn quoted_parent_keeps_one_loose_child_list() {
    assert_eq!(
        carve::to_html("- a\n  > x\n  - t\n\n  - r\n"),
        "<ul>\n  <li>a\n    <blockquote><p>x</p></blockquote>\n    <ul>\n      <li><p>t</p></li>\n      <li><p>r</p></li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn three_blanks_still_split_the_child_lists() {
    let html = carve::to_html("- a\n  > x\n  - t\n\n\n\n  - r\n");
    assert_eq!(html.matches("    <ul>").count(), 2, "{html}");
}
