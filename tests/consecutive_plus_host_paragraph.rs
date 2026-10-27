#[test]
fn refused_second_plus_starts_a_paragraph_in_quote_and_description() {
    assert_eq!(
        carve::to_html("> q\n+\n+"),
        "<blockquote>\n  <p>q</p>\n  <p>+</p>\n</blockquote>"
    );
    assert_eq!(
        carve::to_html(":: t\n: q\n+\n+"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>q</p>\n    <p>+</p>\n  </dd>\n</dl>"
    );
}

#[test]
fn refused_second_plus_stays_in_the_footnote_body() {
    let html = carve::to_html("[^a]: q\n+\n+\n\nref [^a]");
    assert!(html.contains("<p>q</p>\n      <p>+<a"), "{html}");
    assert!(!html.starts_with("<p>+</p>"), "{html}");
}
