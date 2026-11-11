fn note_html(body: &str) -> String {
    carve::to_html(&format!("> [^n]: b\n{body}\n\nx[^n]\n"))
}

#[test]
fn a_lazy_comment_opener_can_close_at_the_quoted_note_body_column() {
    let html = note_html("> {% c\n>   z %}");
    assert!(html.contains("<p>b\n<a href=\"#fnref1\""), "{html}");
    assert!(!html.contains("{% c"), "{html}");
}

#[test]
fn a_comment_continuation_short_of_the_body_column_stays_literal() {
    let html = note_html("> {% c\n>  z %}");
    assert!(
        html.contains("<p>b\n{% c\nz %}<a href=\"#fnref1\""),
        "{html}"
    );
}

#[test]
fn a_comment_continuation_without_the_quote_prefix_stays_literal() {
    let html = note_html("> {% c\n  z %}");
    assert!(
        html.contains("<p>b\n{% c\nz %}<a href=\"#fnref1\""),
        "{html}"
    );
}
