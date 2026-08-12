use carve::ast::{BlockNode, InlineNode};

fn first_paragraph_types(source: &str) -> Vec<&'static str> {
    let document = carve::parse(source);
    let BlockNode::Paragraph(paragraph) = &document.children[0] else {
        panic!("first block is not a paragraph");
    };
    paragraph
        .children
        .iter()
        .map(|node| match node {
            InlineNode::Text(_) => "text",
            InlineNode::SoftBreak(_) => "soft_break",
            InlineNode::Footnote(_) => "footnote_ref",
            _ => "other",
        })
        .collect()
}

#[test]
fn a_reference_label_does_not_cross_any_line_ending() {
    for ending in ["\n", "\r\n", "\r"] {
        let source = format!("before[^two{ending}words].\n");
        assert_eq!(
            first_paragraph_types(&source),
            ["text", "soft_break", "text"]
        );
        assert!(!carve::to_html(&source).contains("doc-noteref"));
    }
}

#[test]
fn a_multiline_definition_marker_does_not_register() {
    let source = "see[^two words].\n\n[^two\nwords]: note.\n";
    let document = carve::parse(source);
    assert!(document.footnote_defs.is_empty());
    assert!(!carve::to_html(source).contains("doc-endnotes"));
    assert_eq!(carve::to_carve(source), source);
}

#[test]
fn same_line_spaces_and_tabs_still_resolve_exactly() {
    for label in ["two words", "two\twords"] {
        let source = format!("see[^{label}].\n\n[^{label}]: note.\n");
        assert!(carve::to_html(&source).contains("doc-noteref"));
    }
}
