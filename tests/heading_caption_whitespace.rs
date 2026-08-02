//! §756 (NORMATIVE): a block's FINAL line trailing whitespace is stripped
//! before rendering; interior trailing (before a soft break) is kept. A leading
//! TAB after the `#`/`^` + space delimiter is content (only leading SPACES fold
//! into the delimiter). Headings and captions share this rule. NBSP is content
//! and is never stripped. Matches carve-js and carve-php.

#[test]
fn heading_strips_final_trailing_whitespace() {
    assert_eq!(
        carve::to_html("# x "),
        "<section id=\"x\">\n  <h1>x</h1>\n</section>"
    );
    assert_eq!(
        carve::to_html("# x\t"),
        "<section id=\"x\">\n  <h1>x</h1>\n</section>"
    );
}

#[test]
fn heading_keeps_leading_tab_as_content() {
    assert_eq!(
        carve::to_html("# \tx"),
        "<section id=\"x\">\n  <h1>\tx</h1>\n</section>"
    );
}

#[test]
fn heading_strips_its_trailing_whitespace() {
    // A heading is one line, so §756 has a single line to strip and the line
    // beneath is a paragraph (which strips its own trailing run).
    let expected = "<section id=\"a\">\n  <h1>a</h1>\n  <p>b</p>\n</section>";
    assert_eq!(carve::to_html("# a \nb"), expected);
    assert_eq!(carve::to_html("# a\nb "), expected);
}

#[test]
fn caption_strips_final_trailing_whitespace() {
    assert_eq!(
        carve::to_html("![a](/u)\n^ x "),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>x</figcaption>\n</figure>"
    );
}

#[test]
fn caption_keeps_leading_tab_as_content() {
    assert_eq!(
        carve::to_html("![a](/u)\n^ \tx"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>\tx</figcaption>\n</figure>"
    );
}
