#[test]
fn wrapped_marker_attributes_leave_flush_tail_outside_item() {
    for closer in [".b}", "  .b}"] {
        let source = format!("- {{.a\n{closer}\ntail");
        assert_eq!(
            carve::to_html(&source),
            "<ul>\n  <li></li>\n</ul>\n<p>tail</p>",
            "{source}"
        );
    }
}

#[test]
fn wrapped_marker_attributes_take_a_follower_at_the_content_column() {
    let source = "- {.a\n.b}\n  tail";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li><p class=\"a b\">tail</p></li>\n</ul>"
    );
}
