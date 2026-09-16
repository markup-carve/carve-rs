//! E2a names a link destination and an autolink opaque to a bare closer,
//! alongside code spans and braced inlines (markup-carve/carve#2046). The link
//! LABEL is not, and a raw inline's format token is opaque only as itself
//! (markup-carve/carve-rs#1652).

fn html(src: &str) -> String {
    carve::to_html(src).trim_end().to_string()
}

#[test]
fn an_emphasis_closes_past_a_link_whose_url_holds_its_marker() {
    assert_eq!(
        html("/see [x](http://a.b/c) now/\n"),
        "<p><em>see <a href=\"http://a.b/c\">x</a> now</em></p>"
    );
}

#[test]
fn an_emphasis_closes_past_an_autolink_whose_url_holds_its_marker() {
    assert_eq!(
        html("/see <http://a.b/c> now/\n"),
        "<p><em>see <a href=\"http://a.b/c\">http://a.b/c</a> now</em></p>"
    );
}

#[test]
fn a_marker_inside_a_destination_does_not_close() {
    assert_eq!(html("~[a](b~) c\n"), "<p>~<a href=\"b~\">a</a> c</p>");
}

#[test]
fn a_marker_inside_an_autolink_does_not_close() {
    assert_eq!(
        html("~<http://x/a~>\n"),
        "<p>~<a href=\"http://x/a~\">http://x/a~</a></p>"
    );
}

#[test]
fn an_image_destination_is_opaque_too() {
    assert_eq!(
        html("~see ![a](b~) now~\n"),
        "<p><s>see <img src=\"b~\" alt=\"a\"> now</s></p>"
    );
}

#[test]
fn a_title_inside_the_destination_is_opaque_too() {
    assert_eq!(
        html("~see [a](b \"t~\") now~\n"),
        "<p><s>see <a href=\"b\" title=\"t~\">a</a> now</s></p>"
    );
}

#[test]
fn parentheses_inside_the_destination_balance() {
    assert_eq!(
        html("~see [a](b(c~)d) now~\n"),
        "<p><s>see <a href=\"b(c~)d\">a</a> now</s></p>"
    );
}

#[test]
fn a_link_label_still_closes() {
    assert_eq!(html("~[a~](b)\n"), "<p><s>[a</s>](b)</p>");
}

#[test]
fn a_destination_with_no_bracket_of_its_own_still_closes() {
    assert_eq!(html("~a](b~) c\n"), "<p><s>a](b</s>) c</p>");
}

#[test]
fn a_destination_holding_a_space_still_closes() {
    assert_eq!(html("~[a](b c~) d~\n"), "<p><s>[a](b c</s>) d~</p>");
}

#[test]
fn a_footnote_reference_is_no_link() {
    assert_eq!(html("~[^n](b~) c~\n"), "<p><s>[^n](b</s>) c~</p>");
}

#[test]
fn an_angle_wrapped_destination_still_closes() {
    assert_eq!(html("~[a](<b~>) c\n"), "<p><s>[a](&lt;b</s>&gt;) c</p>");
}

#[test]
fn an_empty_destination_still_closes() {
    assert_eq!(html("~[a]() c~\n"), "<p><s>[a]() c</s></p>");
}

#[test]
fn a_raw_inline_is_opaque_only_as_its_own_token() {
    // The closer at `c=` is reachable: the main loop builds the code span and
    // the format token as one node, so no highlight opens at `{=`.
    assert_eq!(
        html("=a `b`{=html} c= d=}\n"),
        "<p><mark>a b c</mark> d=}</p>"
    );
}

#[test]
fn a_brace_that_is_not_a_raw_inline_stays_a_braced_inline() {
    assert_eq!(
        html("=a {=html} c= d=}\n"),
        "<p>=a <mark>html} c= d</mark></p>"
    );
}
