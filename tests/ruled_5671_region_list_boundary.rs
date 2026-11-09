use carve::to_html;

#[test]
fn a_region_marker_at_list_column_splits_matching_items() {
    assert_eq!(
        to_html("- a\n%% #region x\n- b\n").trim(),
        "<ul>\n  <li>a</li>\n</ul>\n<ul>\n  <li>b</li>\n</ul>"
    );
}
