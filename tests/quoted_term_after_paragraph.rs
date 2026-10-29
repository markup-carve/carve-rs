#[test]
fn term_after_colon_paragraph_keeps_lazy_line() {
    assert_eq!(
        carve::to_html("> : d\n> :: c\n  # H"),
        "<blockquote>\n  <p>: d</p>\n  <dl>\n    <dt>c</dt>\n  </dl>\n</blockquote>\n<section id=\"H\">\n  <h1>H</h1>\n</section>"
    );
}

#[test]
fn term_after_plain_paragraph_keeps_lazy_line() {
    assert_eq!(
        carve::to_html("> text\n> :: c\n  # H"),
        "<blockquote>\n  <p>text</p>\n  <dl>\n    <dt>c</dt>\n  </dl>\n</blockquote>\n<section id=\"H\">\n  <h1>H</h1>\n</section>"
    );
}
