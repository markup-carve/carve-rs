//! PART 8 resolves autolinks and cross-references before links and emphasis,
//! and PART 9 E2a makes both opaque to delimiter pairing.

fn html(src: &str) -> String {
    carve::to_html(src).trim_end().to_string()
}

#[test]
fn a_bracket_inside_an_autolink_opens_no_run() {
    assert_eq!(
        html("<https://a[> *q](u*) z\n"),
        "<p><a href=\"https://a[\">https://a[</a> <strong>q](u</strong>) z</p>"
    );
}

#[test]
fn a_bracket_inside_a_cross_reference_opens_no_run() {
    assert_eq!(
        html("</#a[> *q](u*) z\n"),
        "<p>&lt;/#a[&gt; <strong>q](u</strong>) z</p>"
    );
}

#[test]
fn an_opener_before_the_autolink_closes_past_it() {
    assert_eq!(
        html("*p <https://a[> *q](u*) z\n"),
        "<p><strong>p <a href=\"https://a[\">https://a[</a> *q](u</strong>) z</p>"
    );
}

#[test]
fn a_real_destination_after_a_code_span_is_still_hidden() {
    assert_eq!(
        html("*a `<https://q` [x](u*) b\n"),
        "<p>*a <code>&lt;https://q</code> <a href=\"u*\">x</a> b</p>"
    );
}

#[test]
fn a_braced_closer_inside_a_cross_reference_does_not_close() {
    assert_eq!(
        html("{*a </#x*}> b*}\n"),
        "<p><strong>a &lt;/#x*}&gt; b</strong></p>"
    );
    assert_eq!(
        html("{/a </#x/}> b/}\n"),
        "<p><em>a &lt;/#x/}&gt; b</em></p>"
    );
    assert_eq!(
        html("{+a </#x+}> b+}\n"),
        "<p><ins>a &lt;/#x+}&gt; b</ins></p>"
    );
}

#[test]
fn an_escaped_cross_reference_hides_nothing() {
    assert_eq!(
        html("{*a \\</#x*}> b*}\n"),
        "<p><strong>a &lt;/<span class=\"tag\"><strong>#x</strong></span></strong>&gt; b*}</p>"
    );
}

#[test]
fn a_substitution_separator_overlapping_a_reference_is_no_arrow() {
    // Ruled 2026-10-15 (markup-carve/carve#3244): the reference is atomic.
    assert_eq!(
        html("{~a </#x~>y> b~}\n"),
        "<p><s>a &lt;/#x~&gt;y&gt; b</s></p>"
    );
}
