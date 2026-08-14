//! A tight list item imported from Markdown holds ONE paragraph, not one per
//! inline node.
//!
//! `pulldown-cmark` spells a tight item by emitting its inlines with NO
//! `Start(Paragraph)` around them - that absence IS the tightness. `Builder`
//! had no inline frame open in that state, so its fallback arm wrapped EACH
//! arriving node in a paragraph of its own and
//!
//! ```text
//! - a *b* c
//! ```
//!
//! came back as
//!
//! ```text
//! - a
//! +
//! /b/
//! +
//!  c
//! ```
//!
//! Every expectation below was measured against `commonmark` 0.31.2, installed
//! outside this checkout, and never against this engine's own output. The
//! reference reads the input above as one `<li>` holding one inline run:
//!
//! ```html
//! <ul>
//! <li>a <em>b</em> c</li>
//! </ul>
//! ```
//!
//! The ticket named the tight item. The rule is wider than that: the fallback
//! fires wherever an inline node arrives with no inline frame open, and an
//! IMAGE ALT is the other place it can happen. There the failure was worse than
//! fragmentation - the construct left the image entirely and landed as a
//! top-level paragraph AHEAD of it, taking its text out of the alt:
//! `![a *b* c](i.png)` imported as `/b/` followed by `![a  c](i.png)`, against
//! the reference's `alt="a b c"`.
//!
//! Reported as `markup-carve/carve-rs#969`.

use carve::{BlockNode, Document};

fn carve(markdown: &str) -> String {
    carve::markdown_to_carve(markdown)
}

fn node_name(node: &BlockNode) -> &'static str {
    match node {
        BlockNode::Paragraph(_) => "paragraph",
        BlockNode::Heading(_) => "heading",
        BlockNode::BlockQuote(_) => "block_quote",
        BlockNode::List(_) => "list",
        BlockNode::CodeBlock(_) => "code_block",
        BlockNode::RawBlock(_) => "raw_block",
        BlockNode::ThematicBreak(_) => "thematic_break",
        _ => "other",
    }
}

/// The blocks of the document's first list's first item, in order.
fn first_item_blocks(doc: &Document) -> Vec<&'static str> {
    let BlockNode::List(list) = &doc.children[0] else {
        panic!("the first child is a list");
    };
    list.items[0].children.iter().map(node_name).collect()
}

// ---- the reported shape ----

/// The item is one paragraph, and the paragraph holds the whole run: text,
/// emphasis, text.
#[test]
fn a_tight_item_holds_one_paragraph_carrying_the_whole_run() {
    let doc = carve::markdown_to_ast("- a *b* c\n");
    assert_eq!(first_item_blocks(&doc), ["paragraph"]);

    let BlockNode::List(list) = &doc.children[0] else {
        panic!("the first child is a list");
    };
    let BlockNode::Paragraph(paragraph) = &list.items[0].children[0] else {
        panic!("the item's only block is a paragraph");
    };
    assert_eq!(paragraph.children.len(), 3, "{:?}", paragraph.children);

    assert_eq!(carve("- a *b* c\n"), "- a /b/ c\n");
}

/// Not about emphasis: every inline construct the ticket measured, and inline
/// HTML, which the importer keeps as text.
#[test]
fn every_inline_construct_stays_in_the_one_paragraph() {
    for (markdown, expected) in [
        ("- a *b* c\n", "- a /b/ c\n"),
        ("- a **b** c\n", "- a *b* c\n"),
        ("- a `b` c\n", "- a `b` c\n"),
        ("- a [t](u) c\n", "- a [t](u) c\n"),
        ("- a <span>b</span> c\n", "- a <span>b</span> c\n"),
        ("- a ~~b~~ c\n", "- a ~b~ c\n"),
    ] {
        let doc = carve::markdown_to_ast(markdown);
        assert_eq!(first_item_blocks(&doc), ["paragraph"], "{markdown:?}");
        assert_eq!(carve(markdown), expected, "{markdown:?}");
    }
}

/// A soft break inside a tight item is part of the same run. It used to split
/// the item into a paragraph on either side of a break that then rendered as a
/// `+` line of its own.
#[test]
fn a_soft_break_stays_inside_the_run() {
    let doc = carve::markdown_to_ast("- a\n  b\n");
    assert_eq!(first_item_blocks(&doc), ["paragraph"]);
    assert_eq!(carve("- a\n  b\n"), "- a\n  b\n");
}

/// An ordered item and a task item take the same path, so neither is left
/// behind by a fix aimed at the bullet the ticket happened to write.
#[test]
fn the_ordered_and_task_spellings_take_the_same_path() {
    assert_eq!(carve("1. a *b* c\n"), "1. a /b/ c\n");
    assert_eq!(carve("- [ ] a *b* c\n"), "- [ ] a /b/ c\n");
    assert_eq!(carve("- [x] a *b* c\n"), "- [x] a /b/ c\n");
}

/// The run belongs to the item that opened it. A nested list opens a SECOND
/// item while the first one's run is still unflushed, so a single buffer on
/// the builder would mix the two, and a run flushed too late would sort behind
/// the list it introduces.
#[test]
fn a_nested_list_does_not_sort_ahead_of_the_text_that_introduces_it() {
    let doc = carve::markdown_to_ast("- a *b* c\n  - d\n");
    assert_eq!(first_item_blocks(&doc), ["paragraph", "list"]);
    assert_eq!(carve("- a *b* c\n  - d\n"), "- a /b/ c\n  - d\n");

    // Two levels, each with its own run, so the buffers cannot be shared.
    assert_eq!(
        carve("- a *b* c\n  - d *e* f\n    - g *h* i\n"),
        "- a /b/ c\n  - d /e/ f\n    - g /h/ i\n"
    );
}

// ---- the controls the ticket named ----

/// A LOOSE item was never affected: the parser wraps its content in a
/// paragraph, so the run had a frame to land in. Pinned so the fix cannot be
/// read as having introduced the paragraph a loose item already had.
#[test]
fn a_loose_item_is_unchanged() {
    let doc = carve::markdown_to_ast("- a *b* c\n\n- d\n");
    assert_eq!(first_item_blocks(&doc), ["paragraph"]);
    assert_eq!(carve("- a *b* c\n\n- d\n"), "- a /b/ c\n\n- d\n");

    // An item holding two paragraphs keeps both - the fix joins a RUN, not
    // every block an item holds.
    let doc = carve::markdown_to_ast("- a\n\n  b\n");
    assert_eq!(first_item_blocks(&doc), ["paragraph", "paragraph"]);
    assert_eq!(carve("- a\n\n  b\n"), "- a\n\n  b\n");
}

/// An item whose content is a single inline node was already correct, and a
/// block quote and a table cell were never on this path at all.
#[test]
fn the_shapes_that_were_already_right_stay_right() {
    assert_eq!(carve("- plain text only\n"), "- plain text only\n");
    assert_eq!(carve("> a *b* c\n"), "> a /b/ c\n");
    assert_eq!(
        carve("| a *b* c | d |\n|---|---|\n| 1 | 2 |\n"),
        "|=a /b/ c|=d|\n| 1 | 2 |\n"
    );
}

/// Tightness itself is not disturbed. The `loose` flag reads
/// `Start(Paragraph)` and the writer spells a tight list without blank lines;
/// both spellings survive the change.
#[test]
fn tightness_survives_the_join() {
    assert_eq!(carve("- a\n- b\n"), "- a\n- b\n");
    assert_eq!(carve("- a\n\n- b\n"), "- a\n\n- b\n");
}

// ---- the same rule, in an image alt ----

/// The construct used to leave the image and land ahead of it as a top-level
/// paragraph, with its text missing from the alt. The reference flattens it
/// into the alt instead.
#[test]
fn a_construct_inside_an_image_alt_stays_in_the_alt() {
    for (markdown, expected) in [
        ("![a *b* c](i.png)\n", "![a b c](i.png)\n"),
        ("![a **b** c](i.png)\n", "![a b c](i.png)\n"),
        ("![a `b` c](i.png)\n", "![a b c](i.png)\n"),
        ("![a [t](u) c](i.png)\n", "![a t c](i.png)\n"),
        ("![a ![n](n.png) c](i.png)\n", "![a n c](i.png)\n"),
    ] {
        let doc = carve::markdown_to_ast(markdown);
        assert_eq!(
            doc.children.iter().map(node_name).collect::<Vec<_>>(),
            ["paragraph"],
            "{markdown:?} must not put anything beside the image"
        );
        assert_eq!(carve(markdown), expected, "{markdown:?}");
    }
}

/// The anchor for the case above: an image with a PLAIN alt was always right,
/// so the assertions there report the constructs being folded in rather than
/// the image having stopped carrying an alt at all.
#[test]
fn a_plain_image_alt_is_unchanged() {
    assert_eq!(carve("![a b c](i.png)\n"), "![a b c](i.png)\n");
}
