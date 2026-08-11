#[test]
fn a_single_table_row_on_a_list_marker_line_is_a_nested_table() {
    let html = carve::to_html("- | - |\n");
    assert!(html.contains("<table>"), "{html}");
    assert!(html.contains("<td>-</td>"), "{html}");
}

#[test]
fn a_whitespace_only_one_cell_row_is_literal() {
    assert!(carve::to_html("* | |\n").contains("<li>| |</li>"));
}

#[test]
fn a_reference_definition_on_a_list_marker_line_is_invisible() {
    let html = carve::to_html("r. +\n");
    assert!(html.contains("<li></li>"), "{html}");
    assert!(!html.contains("[f]"), "{html}");
}

#[test]
fn a_single_line_marker_block_keeps_the_following_item_content() {
    assert!(carve::to_html("- | b |\n  b\n").contains("</table>\n    b"));
    assert!(carve::to_html("* g\n\n[f]: .\n").contains("<li>g</li>"));
    assert!(carve::to_html("- ---\n  y\n").contains("<hr>\n    y"));
}

#[test]
fn a_fence_opener_accepts_a_tab_at_the_end_of_its_line() {
    // This pair used to assert the REFUSAL. markup-carve/carve#1295 ruled the
    // question by POSITION rather than by construct: a tab BEFORE content is
    // the marker-to-content separator and still opens nothing, but a tab at the
    // END of the line with nothing after it is trailing whitespace on a content
    // line, PART 2 drops it, and what is left is the bare opener.
    //
    // The refusal these lines pinned was therefore the separator's answer given
    // to a line that never reaches the separator. The row that did not move is
    // asserted underneath, and `a_fence_opener_drops_its_trailing_tab.rs` holds
    // the full pair with its controls.
    assert!(carve::to_html("```\t\n").contains("<pre><code>"));
    assert!(carve::to_html("~~~\t\n").contains("<pre><code>"));
    // Unchanged: a tab in front of the info string is the separator, so the
    // backtick run is an ordinary inline verbatim run.
    assert!(!carve::to_html("```\tphp\nx\n```\n").contains("<pre><code>"));
}

#[test]
fn a_lazy_ordered_marker_does_not_expose_a_reference_definition() {
    assert_eq!(carve::to_html("r\n. [f]: t\n"), "<p>r\n. [f]: t</p>");
}

#[test]
fn an_unclosed_fence_in_a_paragraph_does_not_hide_a_later_definition() {
    assert_eq!(carve::to_html(":\n``\n"), "<p>:\n<code></code></p>");
}

#[test]
fn a_marker_line_attribute_floats_to_the_following_item_block() {
    assert!(carve::to_html("* {i=\"\"}\n  |\n").contains("<li><p i=\"\">|</p></li>"));
}

#[test]
fn visible_autolink_text_contributes_to_heading_keys() {
    let html = carve::to_html("# a <https://e.com> b\n\n[a <https://e.com> b][]\n");
    assert!(html.contains("id=\"a-https-e-com-b\""), "{html}");
    assert!(html.contains("href=\"#a-https-e-com-b\""), "{html}");
}

#[test]
fn an_empty_heading_key_does_not_resolve_through_the_fallback_slug() {
    let html = carve::to_html("# :smile:\n\n[:smile:][]\n");
    assert!(html.contains("id=\"s\""), "{html}");
    assert!(!html.contains("href=\"#s\""), "{html}");
}
