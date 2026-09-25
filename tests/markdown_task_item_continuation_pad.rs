//! A task item pads its continuation lines to the item's CONTENT COLUMN.
//!
//! The writer used to pad by the whole marker it printed, and on a task item
//! that marker is `- [x] `, six columns. CommonMark's content column for the
//! item is two: the checkbox is the first inline of the item's first paragraph,
//! not part of the marker. Every block below the first paragraph therefore
//! landed four columns past where a reader looks for it, and the block did not
//! reach the output in either spelling - as indented code with a blank above it,
//! as paragraph text without one (carve-rs#1912, carve-rs#1929).
//!
//! The emitted bytes are not the property under test. Each case reads its own
//! output back through `markdown_to_ast`, which follows cmark-gfm
//! 0.29.0.gfm.13, and asserts the block survived.

use carve::ast::{BlockNode, List};

fn first_list(blocks: &[BlockNode]) -> &List {
    blocks
        .iter()
        .find_map(|block| match block {
            BlockNode::List(list) => Some(list),
            _ => None,
        })
        .expect("a list at the top level")
}

/// The blocks the first item of the first list holds after a round trip through
/// the Markdown target and back.
fn item_blocks(source: &str) -> Vec<BlockNode> {
    let markdown = carve::to_markdown(source);
    let back = carve::markdown_import::markdown_to_ast(&markdown);
    first_list(&back.children).items[0].children.clone()
}

#[test]
fn a_heading_below_a_task_item_reaches_the_output() {
    let source = "- [x] a\n  # h\n";
    assert_eq!(carve::to_markdown(source), "- [x] a\n  # h\n");
    assert!(
        item_blocks(source)
            .iter()
            .any(|block| matches!(block, BlockNode::Heading(_))),
        "the heading survived: {:?}",
        item_blocks(source)
    );
}

#[test]
fn a_fence_below_a_task_item_reaches_the_output() {
    let source = "- [ ] a\n  ``` rust\n  let x = 1;\n  ```\n";
    assert!(
        item_blocks(source)
            .iter()
            .any(|block| matches!(block, BlockNode::CodeBlock(_))),
        "the fence survived: {}",
        carve::to_markdown(source)
    );
}

#[test]
fn a_nested_list_below_a_task_item_stays_a_list() {
    let source = "- [ ] a\n  - b\n\n    > q\n";
    assert_eq!(carve::to_markdown(source), "- [ ] a\n  - b\n    > q\n");
    let blocks = item_blocks(source);
    let nested = blocks
        .iter()
        .find_map(|block| match block {
            BlockNode::List(list) => Some(list),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the nested list survived: {blocks:?}"));
    assert!(
        nested.items[0]
            .children
            .iter()
            .any(|block| matches!(block, BlockNode::BlockQuote(_))),
        "the quote inside it survived: {:?}",
        nested.items[0].children
    );
}

#[test]
fn a_task_item_holding_only_a_paragraph_is_unchanged() {
    assert_eq!(
        carve::to_markdown("- [ ] todo\n- [x] done\n"),
        "- [ ] todo\n- [x] done\n"
    );
}

#[test]
fn a_bullet_item_with_no_checkbox_keeps_its_two_column_pad() {
    // The control: a plain bullet's printed prefix already ends at the content
    // column, so nothing about it moves.
    assert_eq!(carve::to_markdown("- a\n  # h\n"), "- a\n  # h\n");
}

#[test]
fn an_ordered_item_keeps_the_width_of_its_whole_marker() {
    // There the whole prefix IS the marker, so the pad stays the marker's width.
    assert_eq!(carve::to_markdown("1. a\n   # h\n"), "1. a\n   # h\n");
    assert!(
        item_blocks("1. a\n   # h\n")
            .iter()
            .any(|block| matches!(block, BlockNode::Heading(_))),
        "the heading survived"
    );
}
