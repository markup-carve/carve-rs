//! A mention or tag directly against a word character has no spelling, so the
//! writer refuses the tree (ruling markup-carve/carve-js#1807,
//! markup-carve/carve-rs#1729).

use carve::{parse, render_carve, BlockNode, Document, InlineNode, RenderCarveError};

/// Parse `source`, whose paragraph is text, a mention or tag, and text, then set
/// the two text values.
fn tree(source: &str, before: &str, after: &str) -> Document {
    let mut doc = parse(source);
    let BlockNode::Paragraph(paragraph) = &mut doc.children[0] else {
        panic!("expected a paragraph");
    };
    let [InlineNode::Text(first), _, InlineNode::Text(last)] = &mut paragraph.children[..] else {
        panic!("expected text, a sigil node and text");
    };
    first.value = before.to_string();
    last.value = after.to_string();
    doc
}

fn refused(doc: &Document) -> bool {
    matches!(
        render_carve(doc),
        Err(RenderCarveError::SourceUnspellable(_))
    )
}

#[test]
fn a_letter_before_a_mention() {
    assert!(refused(&tree("x @name y\n", "xa", " y")));
}

#[test]
fn a_letter_after_a_mention() {
    assert!(refused(&tree("x @name y\n", "x ", "ax")));
}

#[test]
fn an_underscore_before_a_tag() {
    assert!(refused(&tree("x #tag y\n", "x_", " y")));
}

#[test]
fn a_digit_after_a_tag() {
    assert!(refused(&tree("x #tag y\n", "x ", "1")));
}

/// Control: punctuation on both sides is written and reads back as the tree.
#[test]
fn punctuation_on_both_sides_is_written() {
    let doc = tree("x @name y\n", "x.", ".y");
    assert_eq!(render_carve(&doc).unwrap(), "x.@name\\.y\n");
}

/// Control: an underscore after the name has an escape.
#[test]
fn an_underscore_after_the_name_is_written() {
    let doc = tree("x @name y\n", "x ", "_y");
    assert_eq!(render_carve(&doc).unwrap(), "x @name\\_y\n");
}
