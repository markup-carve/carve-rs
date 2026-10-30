#[test]
fn nested_quote_opens_after_outer_content() {
    let html = carve::to_html("> outer\n> > +\n```\nx\n```");
    assert!(html.contains("<p>outer</p>"), "{html}");
    assert!(
        html.contains(
            "<blockquote><p>+</p></blockquote>\n</blockquote>\n<pre><code>x\n</code></pre>"
        ),
        "{html}"
    );
}

#[test]
fn an_open_inner_quote_keeps_plus_as_text() {
    let html = carve::to_html("> > a\nlazy\n> > +\nz");
    assert!(html.contains("a\nlazy\n+\nz"), "{html}");
}

#[test]
fn a_quoted_line_after_the_first_block_is_a_new_quote_paragraph() {
    let html = carve::to_html("> +\nfoo\n> bar");
    assert!(html.contains("<p>foo</p>\n  <p>bar</p>"), "{html}");
}
