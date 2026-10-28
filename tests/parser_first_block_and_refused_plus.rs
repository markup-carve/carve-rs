use carve::to_html;

#[test]
fn refused_first_block_leaves_an_intervening_list_in_the_description() {
    for marker in ["-", "*", "1.", "- [x]"] {
        let source = format!(":: t\n: {marker} +\n   - z\n");
        let html = to_html(&source);
        let full = carve::try_to_html_with_options(&source, &carve::Options::default()).unwrap();
        assert_eq!(html, full, "{source:?}");
        assert!(html.contains("<dd>\n"), "{source:?}: {html}");
        assert!(html.contains("<li>z</li>"), "{source:?}: {html}");
        assert!(!html.contains("<p>- z</p>"), "{source:?}: {html}");
    }
}

#[test]
fn a_document_column_line_does_not_attach_to_a_nested_item() {
    let html = to_html(":: t\n: - +\nx\n");
    assert!(html.contains("</dl>\n<p>x</p>"), "{html}");
}

#[test]
fn a_refused_second_plus_is_text_in_the_attached_item_paragraph() {
    for source in ["> - q\n> +\n> x\n> +\n", "::: box\n- q\n+\nx\n+\n:::\n"] {
        let html = to_html(source);
        assert!(html.contains("x\n+"), "{source:?}: {html}");
        assert!(!html.contains("<p>+</p>"), "{source:?}: {html}");
    }
}
