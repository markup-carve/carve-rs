#[test]
fn a_plus_after_a_lazy_line_attaches_the_next_block_to_the_item() {
    let cases = [
        "- q\nr\n+\nb\n",
        "1. q\nr\n+\nb\n",
        "- [ ] q\nr\n+\nb\n",
        "> - q\n> r\n> +\n> b\n",
        "- - q\n  r\n  +\n  b\n",
    ];
    for source in cases {
        let html = carve::to_html(source);
        assert!(!html.contains('+'), "{source:?}\n{html}");
        assert!(html.contains("r\n"), "{source:?}\n{html}");
        assert!(html.contains("    b\n"), "{source:?}\n{html}");
    }
}

#[test]
fn a_plus_after_a_lazy_quoted_line_attaches_to_the_inner_item() {
    let html = carve::to_html("- > - > q\n  > r\n  > +\n  > b\n");
    assert!(
        html.contains("<blockquote><p>q\nr</p></blockquote>\n          b\n"),
        "{html}"
    );
}
