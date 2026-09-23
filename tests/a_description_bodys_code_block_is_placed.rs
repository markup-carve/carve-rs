//! A fenced code block inside a description body was published with no `pos`
//! at all, and the `definition_description` and `definition_list` around it
//! then ended at their last PLACED child (carve-rs#1833).
//!
//! PART 12 §4's exemption is for a node that CANNOT be placed, not one that has
//! not been placed. The block's opener, body and closer are contiguous source,
//! so an honest span exists; carve-js and carve-php both report it.
//!
//! Two collectors dropped it. A description body manufactures a blank line in
//! front of a fence it refuses to fold, and the blank took the fence line's own
//! column entry; and a line map dropped an unmapped line written before the
//! first mapped one, so a body that opens on that manufactured blank answered
//! for the line above each of its own.

use carve::ast::{BlockNode, Pos};

fn document(source: &str) -> carve::ast::Document {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    carve::parse_with_options(source, &options)
}

fn definition_list(doc: &carve::ast::Document) -> carve::ast::DefinitionList {
    let BlockNode::DefinitionList(list) = &doc.children[0] else {
        panic!("the fixture did not parse as a definition list");
    };
    list.clone()
}

fn code_block_pos(block: &BlockNode) -> Pos {
    let BlockNode::CodeBlock(block) = block else {
        panic!("expected a code block, got {block:?}");
    };
    block
        .pos
        .clone()
        .expect("a fenced code block must carry a position")
}

fn slice(source: &str, pos: &Pos) -> String {
    source
        .chars()
        .skip(pos.start_offset)
        .take(pos.end_offset - pos.start_offset)
        .collect()
}

/// Corpus 450, row 2.
#[test]
fn a_closed_fence_in_a_description_body_carries_its_span() {
    let source = ":: term\n:  definition\n   ```\n   c\n   ```\ntail\n";
    let doc = document(source);
    let list = definition_list(&doc);
    let description = &list.items[0].definitions[0];

    let code = code_block_pos(&description.children[1]);
    assert_eq!(
        (code.start_offset, code.end_offset),
        (25, 40),
        "the span covers the opener, the body and the closer"
    );
    assert_eq!(slice(source, &code), "```\n   c\n   ```");
    assert_eq!((code.start_line, code.end_line), (3, 5));
}

/// The half that hides: the container ends at its last PLACED child, so an
/// unplaced block shortens everything above it without saying so.
#[test]
fn the_containers_around_it_reach_the_closer() {
    let source = ":: term\n:  definition\n   ```\n   c\n   ```\ntail\n";
    let doc = document(source);
    let list = definition_list(&doc);

    let description = list.items[0].definitions[0]
        .pos
        .clone()
        .expect("a description must carry a position");
    assert_eq!((description.start_offset, description.end_offset), (8, 40));

    let whole = list.pos.clone().expect("a list must carry a position");
    assert_eq!((whole.start_offset, whole.end_offset), (0, 40));
}

/// Corpus 450, row 4: the fence sits one container deeper, whose own body opens
/// on the manufactured blank. Its line map used to answer one line late, which
/// reported the block's CONTENT line as its whole extent.
#[test]
fn a_fence_inside_an_admonition_in_the_body_is_placed_too() {
    let source = ":: term\n:  definition\n   ::: note\n   ```\n   :::\n   ```\n   :::\ntail\n";
    let doc = document(source);
    let list = definition_list(&doc);

    let BlockNode::Admonition(note) = &list.items[0].definitions[0].children[1] else {
        panic!("expected an admonition");
    };
    let code = code_block_pos(&note.children[0]);
    assert_eq!((code.start_offset, code.end_offset), (37, 54));
    assert_eq!(slice(source, &code), "```\n   :::\n   ```");
    assert_eq!((code.start_line, code.end_line), (4, 6));
}
