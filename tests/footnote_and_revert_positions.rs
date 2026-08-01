//! Two more classes of unplaced inline content (carve-rs#333).
//!
//! A footnote definition's body is collected out of the document into a map,
//! and that map carried no column information at all, so nothing inside a
//! definition could be placed. An unresolved reference link is rebuilt from its
//! own literal source and was rebuilt without a position, though the
//! replacement covers exactly the span the link did.

use carve::ast::{BlockNode, InlineNode};

fn parse(source: &str) -> carve::ast::Document {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    carve::parse_with_options(source, &options)
}

fn first_text(nodes: &[InlineNode]) -> (String, Option<carve::ast::Pos>) {
    for node in nodes {
        if let InlineNode::Text(t) = node {
            if !t.value.trim().is_empty() {
                return (t.value.clone(), t.pos);
            }
        }
    }
    panic!("no text node found");
}

/// A reverted reference link keeps the span it occupied: `raw_ref` IS the
/// source it reverts to, so the text is still a verbatim slice.
#[test]
fn an_unresolved_reference_keeps_its_span() {
    let source = "[a]:\t/url\n\n[a][]\n";
    let codepoints: Vec<char> = source.chars().collect();

    let doc = parse(source);
    let BlockNode::Paragraph(p) = &doc.children[1] else {
        panic!("expected a paragraph");
    };
    let (value, pos) = first_text(&p.children);
    assert_eq!(value, "[a][]", "the reference did not revert to its source");

    let pos = pos.expect("a reverted reference must keep its position");
    let slice: String = codepoints[pos.start_offset..pos.end_offset]
        .iter()
        .collect();
    assert_eq!(slice, value);
}

/// A definition body is collected into a map rather than left in `children`, so
/// both the column map and the offset pass have to reach it explicitly.
#[test]
fn a_footnote_body_is_placed() {
    let source = "See [^a].\n\n[^a]: note body\n";
    let codepoints: Vec<char> = source.chars().collect();

    let doc = parse(source);
    let body = doc.footnote_defs.get("a").expect("the definition");
    let BlockNode::Paragraph(p) = &body[0] else {
        panic!("expected a paragraph in the definition");
    };

    let (value, pos) = first_text(&p.children);
    let pos = pos.expect("a footnote body's text must carry a position");
    let slice: String = codepoints[pos.start_offset..pos.end_offset]
        .iter()
        .collect();
    assert_eq!(slice, value);
    assert_ne!(
        pos.start_offset, pos.end_offset,
        "the span selects nothing, which reads as present and is not"
    );
}

/// A definition collected from inside a list item must be placed at the
/// DOCUMENT's column, which includes what the container took. Measuring against
/// the prefix-stripped line puts the span two columns early, onto the `]:`.
#[test]
fn a_container_nested_definition_is_placed_at_the_document_column() {
    let source = "See [^a].\n\n- [^a]: note body\n";
    let codepoints: Vec<char> = source.chars().collect();

    let doc = parse(source);
    let body = doc.footnote_defs.get("a").expect("the definition");
    let BlockNode::Paragraph(p) = &body[0] else {
        panic!("expected a paragraph in the definition");
    };

    let (value, pos) = first_text(&p.children);
    let pos = pos.expect("a nested footnote body's text must carry a position");
    let slice: String = codepoints[pos.start_offset..pos.end_offset]
        .iter()
        .collect();
    assert_eq!(slice, value, "the span ignored the list marker's width");
}
