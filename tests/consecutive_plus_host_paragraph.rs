#[test]
fn refused_second_plus_starts_a_paragraph_in_quote_and_description() {
    assert_eq!(
        carve::to_html("> q\n+\n+"),
        "<blockquote><p>q</p></blockquote>"
    );
    assert_eq!(
        carve::to_html(":: t\n: q\n+\n+"),
        "<dl>\n  <dt>t</dt>\n  <dd>q</dd>\n</dl>"
    );
}

#[test]
fn refused_second_plus_stays_in_the_footnote_body() {
    let html = carve::to_html("[^a]: q\n+\n+\n\nref [^a]");
    // The claimed second marker renders nothing (carve#5013).
    assert!(html.contains("<p>q<a"), "{html}");
    assert!(!html.starts_with("<p>+</p>"), "{html}");
}
