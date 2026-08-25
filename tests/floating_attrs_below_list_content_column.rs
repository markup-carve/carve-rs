//! Regression coverage for carve-rs#1373 and the carve#1705 content-column
//! clarification. Marker metadata and task checkboxes contribute no width.

#[test]
fn attributes_attach_to_a_heading_at_the_bullet_content_column() {
    assert_eq!(
        carve::to_html("-{#k} {#h}\n  # h\n"),
        "<ul>\n  <li id=\"k\">\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn an_attribute_only_bullet_item_closes_before_a_below_column_heading() {
    assert_eq!(
        carve::to_html("-{#k} {#h}\n # h\n"),
        "<ul>\n  <li id=\"k\"></li>\n</ul>\n<p># h</p>"
    );
}

#[test]
fn an_attribute_only_ordered_item_closes_before_a_below_column_heading() {
    assert_eq!(
        carve::to_html("1.{#k} {#h}\n  # h\n"),
        "<ol>\n  <li id=\"k\"></li>\n</ol>\n<p># h</p>"
    );
}

#[test]
fn an_attribute_only_task_item_closes_before_a_below_column_heading() {
    assert_eq!(
        carve::to_html("-{#k} [x] {#h}\n # h\n"),
        "<ul>\n  <li id=\"k\"><input type=\"checkbox\" checked disabled> </li>\n</ul>\n<p># h</p>"
    );
}

#[test]
fn ordinary_marker_text_keeps_the_below_column_lazy_fold() {
    assert_eq!(
        carve::to_html("-{#k} text\n # h\n"),
        "<ul>\n  <li id=\"k\">text\n# h</li>\n</ul>"
    );
}
