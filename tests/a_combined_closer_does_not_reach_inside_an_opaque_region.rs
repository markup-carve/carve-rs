//! The combined `/*` closer obeys the same PART 9 E2a boundaries as a bare
//! closer: a `*/` inside a link destination, an attribute block or an autolink
//! is that region's content (markup-carve/carve-rs#3482). A `{^` opener owns its
//! `^`, so the bracket after it is a link, not a note.

fn html(src: &str) -> String {
    carve::to_html(src).trim_end().to_string()
}

#[test]
fn a_combined_closer_inside_a_destination_does_not_close() {
    assert_eq!(
        html("/*[a](u*/) b*/\n"),
        "<p><strong><em><a href=\"u*/\">a</a> b</em></strong></p>"
    );
}

#[test]
fn a_combined_closer_inside_an_image_destination_does_not_close() {
    assert_eq!(
        html("/*![a](u*/) b*/\n"),
        "<p><strong><em><img src=\"u*/\" alt=\"a\"> b</em></strong></p>"
    );
}

#[test]
fn a_combined_closer_inside_a_span_attribute_does_not_close() {
    assert_eq!(
        html("/*[a]{title=\"*/\"} b*/\n"),
        "<p><strong><em><span title=\"*/\">a</span> b</em></strong></p>"
    );
}

#[test]
fn a_combined_closer_inside_a_link_title_does_not_close() {
    assert_eq!(
        html("/*[a](u \"t*/\") b*/\n"),
        "<p><strong><em><a href=\"u\" title=\"t*/\">a</a> b</em></strong></p>"
    );
}

#[test]
fn a_combined_closer_inside_an_autolink_does_not_close() {
    assert_eq!(
        html("/*<https://x.y/*/> b*/\n"),
        "<p><strong><em><a href=\"https://x.y/*/\">https://x.y/*/</a> b</em></strong></p>"
    );
}

#[test]
fn a_combined_closer_inside_an_unattached_brace_group_still_closes() {
    assert_eq!(
        html("/*x{title=\"*/\"} b*/\n"),
        "<p><strong><em>x{title=“</em></strong>”} b*/</p>"
    );
}

#[test]
fn a_combined_token_with_no_closer_outside_a_destination_is_literal() {
    assert_eq!(html("/*[a](u*/)\n"), "<p>/*<a href=\"u*/\">a</a></p>");
}

#[test]
fn a_superscript_closer_inside_a_destination_does_not_close() {
    assert_eq!(
        html("{^[a](u^}) b^}\n"),
        "<p><sup><a href=\"u^}\">a</a> b</sup></p>"
    );
}

#[test]
fn an_unclosed_superscript_opener_leaves_a_note() {
    assert!(html("{^[a](u^})\n").starts_with(
        "<p>{<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a>(u^})</p>"
    ));
}

#[test]
fn a_comment_in_the_combined_token_stops_before_a_closer() {
    assert_eq!(
        html("/*a %% b */c d*/\n"),
        "<p><strong><em>a*/c d</em></strong></p>"
    );
}

#[test]
fn a_space_before_the_combined_closer_leaves_the_token_unclosed() {
    assert_eq!(
        html("/*a */c d*/\n"),
        "<p><em><strong>a */c d</strong></em></p>"
    );
}

#[test]
fn a_line_break_before_the_combined_closer_closes() {
    assert_eq!(
        html("/*a\n*/ b*/\n"),
        "<p><strong><em>a\n</em></strong> b*/</p>"
    );
}

#[test]
fn a_comment_marker_opening_a_span_is_text() {
    assert_eq!(html("*%% b* c\n"), "<p><strong>%% b</strong> c</p>");
}

#[test]
fn a_combined_opener_inside_a_bracket_run_does_not_close_outside_it() {
    assert_eq!(html("[x /*]*/](v)\n"), "<p>[x /*]*/](v)</p>");
}
