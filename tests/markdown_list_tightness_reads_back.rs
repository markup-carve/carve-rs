//! The Markdown target keeps a list's tight/loose distinction (carve#2281).
//!
//! The emitted bytes are not the property under test - the tightness a
//! CommonMark reader takes from them is. So every case below reads its own
//! output back through `markdown_to_ast`, which follows cmark-gfm 0.29.0.gfm.13,
//! and asserts the flag rather than the spelling. `- parent` / `  - child`
//! used to go out with a blank line between the two, and cmark-gfm answers a
//! blank there with `<li><p>parent</p>` where the document's own HTML says
//! `<li>parent`.
//!
//! The separator is not always wrong, which is why the writer asks per case:
//! an ordered marker that does not start at 1 cannot interrupt a paragraph,
//! and a marker with nothing after it reads as a setext underline.

use carve::ast::{BlockNode, Document};

fn first_list(doc: &Document) -> &carve::ast::List {
    fn find(blocks: &[BlockNode]) -> Option<&carve::ast::List> {
        for block in blocks {
            if let BlockNode::List(list) = block {
                return Some(list);
            }
        }
        None
    }
    find(&doc.children).expect("a list at the top level")
}

/// Tightness of the outermost list after the Markdown output is read back.
fn reads_back_tight(source: &str) -> bool {
    let markdown = carve::to_markdown(source);
    let back = carve::markdown_import::markdown_to_ast(&markdown);
    first_list(&back).tight
}

fn tight_in_source(source: &str) -> bool {
    first_list(&carve::parse(source)).tight
}

#[test]
fn a_nested_list_does_not_loosen_the_item_above_it() {
    assert_eq!(
        carve::to_markdown("- parent\n  - child\n"),
        "- parent\n  - child\n"
    );
    assert!(reads_back_tight("- parent\n  - child\n"));
}

#[test]
fn a_nested_quote_does_not_loosen_the_item_above_it() {
    let source = "- a\n  > - x\n\n  - m\n";
    assert!(tight_in_source(source));
    assert_eq!(carve::to_markdown(source), "- a\n  > - x\n  - m\n");
    assert!(reads_back_tight(source));
}

#[test]
fn a_loose_list_reads_back_loose() {
    let source = "- a\n\n- b\n";
    assert!(!tight_in_source(source));
    assert_eq!(carve::to_markdown(source), "- a\n\n- b\n");
    assert!(!reads_back_tight(source));
}

#[test]
fn an_ordered_marker_that_cannot_interrupt_keeps_the_separator() {
    // `3.` below a paragraph line is text to a CommonMark reader, so the blank
    // has to stay or the nested list stops being a list.
    let out = carve::to_markdown("- a\n  3. b\n");
    assert_eq!(out, "- a\n\n  3. b\n");
    let back = carve::markdown_import::markdown_to_ast(&out);
    let outer = first_list(&back);
    assert!(
        matches!(outer.items[0].children.get(1), Some(BlockNode::List(inner)) if inner.ordered),
        "the nested ordered list survived: {out}"
    );
}

#[test]
fn an_ordered_marker_starting_at_one_drops_the_separator() {
    assert_eq!(carve::to_markdown("- a\n  1. b\n"), "- a\n  1. b\n");
    assert!(reads_back_tight("- a\n  1. b\n"));
}

#[test]
fn an_empty_nested_marker_keeps_the_separator() {
    // The nested item holds only a comment, which this target drops, so its
    // marker goes out bare. A lone `-` under a paragraph line is a setext
    // underline, which would turn the item's text into a heading instead.
    let source = "- a\n  - %% c\n";
    let out = carve::to_markdown(source);
    assert_eq!(out, "- a\n\n  -\n");
    let back = carve::markdown_import::markdown_to_ast(&out);
    assert!(
        matches!(
            first_list(&back).items[0].children.get(1),
            Some(BlockNode::List(_))
        ),
        "the nested list survived: {out}"
    );
}

#[test]
fn a_loose_list_keeps_its_looseness_at_the_item_boundary() {
    // The looseness lives in the boundary between the two items and nowhere
    // else, so only a blank line between them can carry it.
    let source = "- a\n\n- b\n\n- c\n";
    assert_eq!(carve::to_markdown(source), "- a\n\n- b\n\n- c\n");
    assert!(!reads_back_tight(source));
}

#[test]
fn a_tight_list_of_plain_items_is_unchanged() {
    assert_eq!(carve::to_markdown("- a\n- b\n"), "- a\n- b\n");
    assert!(reads_back_tight("- a\n- b\n"));
}
