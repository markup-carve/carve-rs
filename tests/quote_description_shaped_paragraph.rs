#[test]
fn description_shaped_text_keeps_quote_paragraph_open() {
    let input = "> :: t\n>  a\n> : :::\nz\n";
    assert_eq!(
        carve::to_html(input),
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n  </dl>\n  <p>a\n: :::\nz</p>\n</blockquote>"
    );
}

#[test]
fn comment_span_text_after_ended_term_keeps_quote_paragraph_open() {
    let input = "> :: t\n>  a\n> : %%%\nz\n";
    assert_eq!(
        carve::to_html(input),
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n  </dl>\n  <p>a\n:\nz</p>\n</blockquote>"
    );
}
