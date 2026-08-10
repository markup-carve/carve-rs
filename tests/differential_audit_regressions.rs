#[test]
fn a_single_table_row_on_a_list_marker_line_is_a_nested_table() {
    let html = carve::to_html("- |-|\n");
    assert!(html.contains("<table>"), "{html}");
    assert!(html.contains("<td>-</td>"), "{html}");
}

#[test]
fn a_whitespace_only_one_cell_row_is_literal() {
    assert!(carve::to_html("* | |\n").contains("<li>| |</li>"));
}

#[test]
fn a_reference_definition_on_a_list_marker_line_is_invisible() {
    let html = carve::to_html("r. [f]: t\n");
    assert!(html.contains("<li></li>"), "{html}");
    assert!(!html.contains("[f]"), "{html}");
}

#[test]
fn a_single_line_marker_block_keeps_the_following_item_content() {
    assert!(carve::to_html("- |b|\nb\n").contains("</table>\n    b"));
    assert!(carve::to_html("* [f]: .\ng\n").contains("<li>g</li>"));
    assert!(carve::to_html("- ---\ny\n").contains("<hr>\n    y"));
}

#[test]
fn a_fence_opener_rejects_a_tab_after_the_run() {
    assert!(!carve::to_html("```\t\n").contains("<pre><code>"));
    assert!(!carve::to_html("~~~\t\n").contains("<pre><code>"));
}

#[test]
fn a_lazy_ordered_marker_does_not_expose_a_reference_definition() {
    assert_eq!(carve::to_html("r\n. [f]: t\n"), "<p>r\n. [f]: t</p>");
}

#[test]
fn an_unclosed_fence_in_a_paragraph_does_not_hide_a_later_definition() {
    assert_eq!(
        carve::to_html(":\n```\n[A]: b\n"),
        "<p>:\n<code></code></p>"
    );
}

#[test]
fn a_marker_line_attribute_floats_to_the_following_item_block() {
    assert!(carve::to_html("* {i}\n|\n").contains("<li><p i=\"\">|</p></li>"));
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
