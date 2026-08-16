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
fn a_single_line_marker_block_ends_the_item_at_the_following_flush_left_line() {
    // These three used to KEEP the following line as item content. PART 1 S4
    // was ruled uniform in markup-carve/carve#1280: lazy continuation extends an
    // open paragraph and nothing else, and a table, a link reference definition
    // and a thematic break each leave none - so the item ends and the line is a
    // top-level block, exactly as the block-quote spelling always did.
    assert!(carve::to_html("- |b|\nb\n").contains("</table>\n  </li>\n</ul>\n<p>b</p>"));
    assert!(carve::to_html("* [f]: .\ng\n").contains("<li></li>\n</ul>\n<p>g</p>"));
    assert!(carve::to_html("- ---\ny\n").contains("<hr>\n  </li>\n</ul>\n<p>y</p>"));
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
fn a_marker_line_attribute_does_not_reach_a_flush_left_block() {
    // It used to pull the flush-left line INTO the item so the attributes had
    // something to attach to. A floating attribute is scoped to the container
    // that holds it (§15 A4, markup-carve/carve#1281) and leaves no open
    // paragraph (markup-carve/carve#1280), so the item ends and the attribute
    // is dropped where it was written.
    assert_eq!(
        carve::to_html("* {i}\n|\n"),
        "<ul>\n  <li></li>\n</ul>\n<p>|</p>"
    );
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
