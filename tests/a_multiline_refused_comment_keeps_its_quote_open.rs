// A refused comment run may contain later brace-starting lines. Its opener
// still determines whether the next line is kept in the quote.
#[test]
fn nested_braces_do_not_displace_the_comment_opener() {
    let source = "> a\n> {% c\n> { d\n> { e\n> f %}\n> # h\nlazy\n";
    assert_eq!(
        carve::to_html(source).trim(),
        "<blockquote><p>a\n{% c\n{ d\n{ e\nf %}\n# h\nlazy</p></blockquote>"
    );
}
