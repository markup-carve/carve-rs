#[test]
fn quoted_link_definitions_in_footnotes_register_document_wide() {
    for source in [
        "[^n]: a\n\n    > [r]: /x\n[x][r]\n",
        "[^n]: a\n  > [r]: /x\n[x][r]\n",
        "[^n]: a\n    > [r]: /x\n[x][r]\n",
    ] {
        let html = carve::to_html(source);
        assert!(html.contains("<a href=\"/x\">x</a>"), "{source:?}: {html}");
    }
}

#[test]
fn a_quoted_paragraph_keeps_a_definition_shaped_line_as_text() {
    let source = "[^n]: a\n  > prose\n  > [r]: /x\n[x][r] [^n]\n";
    let html = carve::to_html(source);
    assert!(html.contains("[x][r]"), "{html}");
    assert!(html.contains("[r]: /x"), "{html}");
}
