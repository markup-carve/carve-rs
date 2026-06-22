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
fn all_space_code_span_does_not_strip_or_panic() {
    assert_eq!(html("` `"), "<p><code> </code></p>");
    assert_eq!(html("`  a  `"), "<p><code> a </code></p>");
}

#[test]
fn adjacent_span_ids_and_classes_are_all_parsed() {
    assert_eq!(html("[a]{#i#j}"), "<p><span id=\"j\">a</span></p>");
    assert_eq!(html("[a]{.a.b}"), "<p><span class=\"a b\">a</span></p>");
}

#[test]
fn unordered_marker_tail_strips_all_leading_whitespace() {
    assert_eq!(html("-   x"), "<ul>\n  <li>x</li>\n</ul>");
}

#[test]
fn changing_unordered_marker_starts_new_list() {
    assert_eq!(
        html("* a\n- b"),
        "<ul>\n  <li>a</li>\n</ul>\n<ul>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn marker_line_blockquote_nests_inside_list_item() {
    assert_eq!(
        html("- > q"),
        "<ul>\n  <li>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn empty_backtick_pair_after_dollar_is_code_not_math() {
    assert_eq!(html("$``"), "<p>$<code></code></p>");
    assert_eq!(html("$$``"), "<p>$$<code></code></p>");
}

#[test]
fn reference_and_footnote_definitions_require_space_after_colon() {
    assert_eq!(html("[a]:u"), "<p>[a]:u</p>");
    assert_eq!(html("[^1]\n\n[^1]:x"), "<p>[^1]</p>\n<p>[^1]:x</p>");
}

#[test]
fn abbreviation_definition_requires_space_after_colon() {
    assert_eq!(html("*[A]:x\n\nA"), "<p>*[A]:x</p>\n<p>A</p>");
}

#[test]
fn tag_after_crossref_opener_with_space_is_a_tag() {
    assert_eq!(
        html("</#a b>"),
        "<p>&lt;/<span class=\"tag\"><strong>#a</strong></span> b&gt;</p>"
    );
}

#[test]
fn smart_typography_tokenizes_overlapping_arrows_and_dashes_left_to_right() {
    assert_eq!(html("->-->"), "<p>→–&gt;</p>");
    assert_eq!(html("--->"), "<p>—&gt;</p>");
}
