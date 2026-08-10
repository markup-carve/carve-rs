#[test]
fn markdown_paragraph_lines_cannot_become_lists() {
    for (source, expected) in [
        ("para\n- tail\n", "para\n\\- tail\n"),
        ("para\n+ tail\n", "para\n\\+ tail\n"),
        ("para\n1. tail\n", "para\n1\\. tail\n"),
        ("para\n1) tail\n", "para\n1\\) tail\n"),
    ] {
        let doc = carve::parse(source);
        assert_eq!(carve::render_markdown(&doc).expect("renders"), expected);
    }

    assert_eq!(
        carve::render_markdown(&carve::parse("- real\n")).expect("renders"),
        "- real\n"
    );
    assert_eq!(
        carve::render_markdown(&carve::parse("para ``code\n- literal``\n")).expect("renders"),
        "para `code\n- literal`\n"
    );
}

#[test]
fn plain_text_indents_each_list_ancestor_by_two_spaces() {
    let source = "- a\n  - b\n    - c\n- d\n";
    let doc = carve::parse(source);
    assert_eq!(carve::render_plain_text(&doc).expect("renders"), source);
}
