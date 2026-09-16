//! An attribute block after a mention or a tag stays literal, so a tree that
//! carries one has no spelling and the writer refuses it (ruling
//! markup-carve/carve-php#2083, markup-carve/carve-rs#1737).

use carve::{
    parse, render_carve, AttrSlot, Attrs, BlockNode, Document, InlineNode, RenderCarveError,
};

/// Parse `source`, whose paragraph holds one mention or tag, and give that node
/// the attributes.
fn tree(source: &str, attrs: Attrs) -> Document {
    let mut doc = parse(source);
    let BlockNode::Paragraph(paragraph) = &mut doc.children[0] else {
        panic!("expected a paragraph");
    };
    match &mut paragraph.children[..] {
        [InlineNode::Mention(mention)] => mention.attrs = Some(attrs),
        [InlineNode::Tag(tag)] => tag.attrs = Some(attrs),
        _ => panic!("expected one mention or tag"),
    }
    doc
}

fn class(name: &str) -> Attrs {
    Attrs {
        classes: vec![name.to_string()],
        order: vec![AttrSlot::Class],
        ..Attrs::default()
    }
}

fn refused(doc: &Document) -> bool {
    matches!(
        render_carve(doc),
        Err(RenderCarveError::SourceUnspellable(_))
    )
}

#[test]
fn a_class_on_a_mention() {
    assert!(refused(&tree("@name\n", class("c"))));
}

#[test]
fn a_class_on_a_tag() {
    assert!(refused(&tree("#x\n", class("c"))));
}

#[test]
fn an_id_on_a_mention() {
    let attrs = Attrs {
        id: Some("k".to_string()),
        order: vec![AttrSlot::Id],
        ..Attrs::default()
    };
    assert!(refused(&tree("@name\n", attrs)));
}

#[test]
fn a_key_value_on_a_tag() {
    let mut attrs = Attrs::default();
    attrs
        .key_values
        .insert("data-k".to_string(), "v".to_string());
    attrs.order.push(AttrSlot::Key("data-k".to_string()));
    assert!(refused(&tree("#x\n", attrs)));
}

/// Control: the node writes as before when the attributes carry nothing.
#[test]
fn an_empty_attribute_set_is_written() {
    assert_eq!(
        render_carve(&tree("@name\n", Attrs::default())).unwrap(),
        "@name\n"
    );
}

/// Control: a mention with no attributes at all is written.
#[test]
fn a_bare_mention_is_written() {
    assert_eq!(render_carve(&parse("x @name y\n")).unwrap(), "x @name y\n");
}

/// Control: the tree still RENDERS, so only the Carve writer refuses it.
#[test]
fn the_refused_tree_still_renders_as_html() {
    assert_eq!(
        carve::render_html(&tree("@name\n", class("c"))).unwrap(),
        "<p><span class=\"mention c\"><strong>@name</strong></span></p>"
    );
}
