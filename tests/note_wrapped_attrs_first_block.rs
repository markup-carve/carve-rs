#[test]
fn a_wrapped_attribute_line_missed_by_a_note_leads_the_items_first_block() {
    assert_eq!(
        carve::to_html("- +\n[^m]: n\n{.x\n .y}\np\n"),
        "<ul>\n  <li><p class=\"x y\">p</p></li>\n</ul>"
    );
}
