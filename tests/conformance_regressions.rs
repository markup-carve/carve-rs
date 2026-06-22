//! Focused cross-implementation conformance regressions.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn empty_unquoted_attribute_value_rejects_whole_block() {
    assert_eq!(html("[a]{k=}"), "<p>[a]{k=}</p>");
}

#[test]
fn empty_link_destination_stays_literal() {
    assert_eq!(html("[]( )"), "<p>[]( )</p>");
}

#[test]
fn blank_separated_indented_footnote_continuation_stays_in_footnote() {
    assert_eq!(
        html("x[^1]\n\n[^1]: a\n\n  b"),
        concat!(
            "<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n",
            "<section role=\"doc-endnotes\">\n",
            "  <hr>\n",
            "  <ol>\n",
            "    <li id=\"fn1\">\n",
            "      <p>a</p>\n",
            "      <p>b<a href=\"#fnref1\" role=\"doc-backlink\">↩</a></p>\n",
            "    </li>\n",
            "  </ol>\n",
            "</section>"
        )
    );
}

#[test]
fn compact_multiple_ids_last_wins() {
    assert_eq!(html("[a]{#i#j}"), "<p><span id=\"j\">a</span></p>");
}

#[test]
fn compact_multiple_classes_accumulate() {
    assert_eq!(
        html("[a]{.a.b}"),
        "<p><span class=\"a b\">a</span></p>"
    );
}

#[test]
fn list_marker_strips_all_whitespace_before_content() {
    assert_eq!(html("-   x"), "<ul>\n  <li>x</li>\n</ul>");
}

#[test]
fn bullet_marker_change_splits_list() {
    assert_eq!(
        html("* a\n- b"),
        "<ul>\n  <li>a</li>\n</ul>\n<ul>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn blockquote_on_list_marker_line_nests_in_item() {
    assert_eq!(
        html("- > q"),
        "<ul>\n  <li>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn empty_code_span_after_dollar_is_not_math() {
    assert_eq!(html("$``"), "<p>$<code></code></p>");
    assert_eq!(html("$$``"), "<p>$$<code></code></p>");
}

#[test]
fn reference_definition_requires_space_after_colon() {
    assert_eq!(html("[a]:u"), "<p>[a]:u</p>");
}

#[test]
fn footnote_definition_requires_space_after_colon() {
    assert_eq!(html("[^1]\n\n[^1]:x"), "<p>[^1]</p>\n<p>[^1]:x</p>");
}
