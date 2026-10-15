use carve::{
    html_to_carve, markdown_to_carve, parse, to_html, to_html_with_options,
    to_markdown_with_options, BlockNode, HtmlImportOptions, InlineNode, Options,
};

fn has_cross_reference(source: &str) -> bool {
    matches!(parse(source).children.first(), Some(BlockNode::Paragraph(p)) if p.children.iter().any(|node| matches!(node, InlineNode::CrossRef(_))))
}

#[test]
fn imported_cross_references_stay_atomic() {
    for (source, markdown_expected, html_expected) in [
        (
            "{~a </#x~>y> b~}\n",
            "~~a </#x~>y> b~~\n",
            "~a </#x~>y> b~\n",
        ),
        ("^[a </#x]> b]\n", "\\^[a </#x]> b]\n", "^[a </#x]> b]\n"),
    ] {
        let options = Options::default().with_roundtrip(true);
        let markdown = to_markdown_with_options(source, &options);
        let from_markdown = markdown_to_carve(&markdown);
        assert_eq!(from_markdown, markdown_expected);

        let html = to_html_with_options(source, &options);
        let from_html = html_to_carve(&html, &HtmlImportOptions::default())
            .expect("HTML import")
            .value;
        assert_eq!(from_html, html_expected);
        assert_eq!(to_html(&from_html), to_html(source));
    }
}

#[test]
fn cross_references_in_imported_text_render_like_the_original() {
    for source in ["a </#x~>y> b\n", "a </#x]> b\n"] {
        let from_markdown = markdown_to_carve(source);
        assert_eq!(from_markdown, source);
        assert!(has_cross_reference(&from_markdown));
        assert_eq!(to_html(&from_markdown), to_html(source));

        let html = to_html(source);
        let from_html = html_to_carve(&html, &HtmlImportOptions::default())
            .expect("HTML import")
            .value;
        assert_eq!(from_html, source);
        assert!(has_cross_reference(&from_html));
        assert_eq!(to_html(&from_html), to_html(source));
    }
}

#[test]
fn escaped_cross_reference_spelling_stays_literal() {
    for source in ["\\</#id>\n", "x \\</#id> y\n"] {
        for roundtrip in [false, true] {
            let options = Options::default().with_roundtrip(roundtrip);
            let markdown = to_markdown_with_options(source, &options);
            let imported = markdown_to_carve(&markdown);
            assert_eq!(
                to_html(&imported),
                to_html(source),
                "Markdown: {markdown:?}, Carve: {imported:?}"
            );

            let html = to_html_with_options(source, &options);
            let imported = html_to_carve(&html, &HtmlImportOptions::default())
                .expect("HTML import")
                .value;
            assert_eq!(to_html(&imported), to_html(source), "HTML: {html:?}");
        }
    }
}
