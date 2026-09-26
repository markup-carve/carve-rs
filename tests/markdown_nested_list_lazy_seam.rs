//! A nested list is the one block this target writes with no blank line behind
//! it, so a block below it was glued whatever the tight-item predicate answered.
//!
//! A table's rows then arrive as lazy continuation of the last nested item and
//! the table does not reach the output at all. A paragraph after an effective
//! `+` belongs to the outer item (carve-rs#1930).
//!
//! Only a block whose Markdown spelling opens with plain text can be taken that
//! way, and only where the tail can take lazy text. A heading, fence, quote or
//! thematic break interrupts on its own. A bare marker or a heading above has
//! no paragraph to continue, so those pairs stay glued and the item stays
//! tight. An inert `+` before indented text leaves the nested paragraph open.
//!
//! Every expectation reads its own output back through `markdown_to_ast`, which
//! follows cmark-gfm 0.29.0.gfm.13. The emitted bytes are asserted only where
//! the point is that they did NOT move.

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

/// The blocks the first item of the first list holds after a round trip.
fn item_blocks(source: &str) -> Vec<BlockNode> {
    let markdown = carve::to_markdown(source);
    let back = carve::markdown_import::markdown_to_ast(&markdown);
    first_list(&back.children).items[0].children.clone()
}

fn holds_a_table(blocks: &[BlockNode]) -> bool {
    blocks
        .iter()
        .any(|block| matches!(block, BlockNode::Table(_)))
}

#[test]
fn a_table_below_a_nested_list_reaches_the_output() {
    let source = "- x\n  - L\n+\n| a |\n|---|\n";
    assert_eq!(
        carve::to_markdown(source),
        "- x\n  - L\n\n  | a |\n  | --- |\n"
    );
    assert!(
        holds_a_table(&item_blocks(source)),
        "the table survived: {:?}",
        item_blocks(source)
    );
}

#[test]
fn an_inert_plus_leaves_plain_text_in_the_deepest_item() {
    // The `+` has no flush-left block to attach. The following line remains
    // lazy continuation of the nested item's paragraph.
    let source = "- x\n  - L\n+\n  p\n";
    assert_eq!(carve::to_markdown(source), "- x\n  - L\n    p\n");
    let blocks = item_blocks(source);
    let nested = blocks
        .iter()
        .find_map(|block| match block {
            BlockNode::List(list) => Some(list),
            _ => None,
        })
        .expect("the nested list");
    assert_eq!(nested.items[0].children.len(), 1);
    assert!(
        !blocks
            .iter()
            .skip_while(|block| !matches!(block, BlockNode::List(_)))
            .skip(1)
            .any(|block| matches!(block, BlockNode::Paragraph(_))),
        "the outer item gained a paragraph: {blocks:?}"
    );
}

#[test]
fn an_opener_below_a_nested_list_keeps_the_item_tight() {
    // The controls. Each of these interrupts on its own, so no blank is written
    // and the outer item does not go loose.
    for (source, expected) in [
        ("- x\n  - L\n+\n# h\n", "- x\n  - L\n  # h\n"),
        (
            "- x\n  - L\n+\n``` r\nc\n```\n",
            "- x\n  - L\n  ```r\n  c\n  ```\n",
        ),
        ("- x\n  - L\n+\n> q\n", "- x\n  - L\n  > q\n"),
        ("- x\n  - L\n+\n---\n", "- x\n  - L\n  ---\n"),
    ] {
        assert_eq!(carve::to_markdown(source), expected);
    }
}

#[test]
fn a_tail_that_cannot_take_lazy_text_keeps_its_paragraph_glued() {
    // A heading inside the nested item ends its paragraph, so the line below is
    // not lazy continuation and the item stays tight.
    assert_eq!(
        carve::to_markdown("- a\n  - b\n    # N\nlazy\n"),
        "- a\n  - b\n    # N\n  lazy\n"
    );
    // An empty nested item has no paragraph to continue.
    assert_eq!(carve::to_markdown("* * +\n  x\n"), "* *\n  x\n");
}

#[test]
fn the_absorption_set_is_unchanged() {
    // carve-rs#1924: two sibling quotes keep their separator and read back as
    // two quotes, and a quote under a paragraph still drops it and reads tight.
    assert_eq!(
        carve::to_markdown("- x\n  > q\n+\n  > r\n"),
        "- x\n  > q\n\n  > r\n"
    );
    assert_eq!(carve::to_markdown("- x\n  > q\n"), "- x\n  > q\n");
}
