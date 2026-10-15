//! PART 9 E2a and CARVE-P3-001 (ruled 2026-10-15): an autolink, a cross-reference
//! and a comment are atomic, so no delimiter, arrow or bracket outside one reads inside it.
//! Corpus category 604 (markup-carve/carve#3244).

fn html(src: &str) -> String {
    carve::to_html(&format!("{src}\n")).trim_end().to_string()
}

#[test]
fn an_autolink_stays_whole_against_the_arrow_that_ends_it() {
    assert_eq!(
        html("{~a <https://x~>y> b~}"),
        "<p><s>a <a href=\"https://x~\">https://x~</a>y&gt; b</s></p>"
    );
}

#[test]
fn a_later_arrow_outside_the_autolink_splits() {
    assert_eq!(
        html("{~a <https://x~>y~>z b~}"),
        "<p><del>a <a href=\"https://x~\">https://x~</a>y</del><ins>z b</ins></p>"
    );
}

#[test]
fn a_cross_reference_stays_whole_against_the_arrow_that_ends_it() {
    assert_eq!(html("{~a </#x~> b~}"), "<p><s>a &lt;/#x~&gt; b</s></p>");
}

#[test]
fn an_address_that_cannot_end_in_a_tilde_lets_its_arrow_split() {
    assert_eq!(
        html("{~a <me@x.org~>y> b~}"),
        "<p><del>a &lt;me@x.org</del><ins>y&gt; b</ins></p>"
    );
}

#[test]
fn an_editorial_comment_hides_an_insertion_closer() {
    assert_eq!(
        html("{+a {#x+}#} b+}"),
        "<p><ins>a <span class=\"critic-comment\">x+}</span> b</ins></p>"
    );
}

#[test]
fn an_editorial_comment_hides_a_deletion_closer() {
    assert_eq!(
        html("{-a {#x-}#} b-}"),
        "<p><del>a <span class=\"critic-comment\">x-}</span> b</del></p>"
    );
}

#[test]
fn a_delimited_comment_hides_an_insertion_closer() {
    assert_eq!(html("{+a {%x+}%} b+}"), "<p><ins>a  b</ins></p>");
}

#[test]
fn a_comment_opened_first_closes_at_its_own_closer() {
    assert_eq!(
        html("{#a </#x#}> b#}"),
        "<p><span class=\"critic-comment\">a &lt;/#x</span>&gt; b#}</p>"
    );
}

#[test]
fn a_link_label_holds_a_cross_reference_with_a_bracket() {
    assert_eq!(
        html("[a </#x]> b](u)"),
        "<p><a href=\"u\">a &lt;/#x]&gt; b</a></p>"
    );
}

#[test]
fn a_span_label_holds_a_cross_reference() {
    assert_eq!(
        html("[a </#x]> b]{.c}"),
        "<p><span class=\"c\">a &lt;/#x]&gt; b</span></p>"
    );
}

#[test]
fn an_image_label_holds_a_cross_reference() {
    assert_eq!(
        html("![a </#x]> b](u)"),
        "<img src=\"u\" alt=\"a &lt;/#x]&gt; b\">"
    );
}

#[test]
fn a_label_holds_a_cross_reference_shaped_like_a_destination() {
    assert_eq!(
        html("[a </#x](u)> b](v)"),
        "<p><a href=\"v\">a &lt;/#x](u)&gt; b</a></p>"
    );
}

#[test]
fn a_link_label_holds_a_delimited_comment() {
    assert_eq!(html("[a {%x]%} b](u)"), "<p><a href=\"u\">a  b</a></p>");
}

#[test]
fn an_inline_note_holds_a_cross_reference() {
    assert_eq!(html("^[a </#x]> b]"), "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>a &lt;/#x]&gt; b<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>");
}

#[test]
fn a_cross_reference_stays_whole_against_an_arrow_before_more_text() {
    assert_eq!(
        html("{~a </#x~>y> b~}"),
        "<p><s>a &lt;/#x~&gt;y&gt; b</s></p>"
    );
}
