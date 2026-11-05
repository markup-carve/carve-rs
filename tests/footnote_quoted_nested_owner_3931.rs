use carve::to_html;

#[test]
fn quoted_nested_definition_takes_its_following_line() {
    let html = to_html("[r][] [^p] [^c] [^d]\n\n[^c]: > para\n  >  [^p]: n\n  > tail\n");
    assert!(html.contains("<p>n\ntail<a href=\"#fnref1\""), "{html}");
    assert!(
        html.contains("<blockquote><p>para</p></blockquote>"),
        "{html}"
    );
    assert!(!html.contains("[^p]: n"), "{html}");
}
