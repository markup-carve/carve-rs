//! PART 7 gives Carve four whitespace characters, and a form feed is not one
//! of them (#796), so it does not guard the combined `/*` opener: `/*<FF>x*/`
//! is bold-italic, as in carve-js. A tab does guard it, as it guards a bare
//! delimiter (CARVE-P3-013).

fn html(src: &str) -> String {
    carve::to_html(src).trim_end().to_string()
}

#[test]
fn a_form_feed_after_the_opener_is_content() {
    assert_eq!(
        html("a /*\u{c}x*/ b"),
        "<p>a <strong><em>\u{c}x</em></strong> b</p>"
    );
}

#[test]
fn a_form_feed_before_the_closer_is_content() {
    assert_eq!(
        html("a /*x\u{c}*/ b"),
        "<p>a <strong><em>x\u{c}</em></strong> b</p>"
    );
}

#[test]
fn a_tab_edged_bold_italic_reads_like_a_space_edged_one() {
    assert_eq!(html("a /*\tx*/ b"), "<p>a <em>*\tx*</em> b</p>");
    assert_eq!(html("a /* x*/ b"), "<p>a <em>* x*</em> b</p>");
}
