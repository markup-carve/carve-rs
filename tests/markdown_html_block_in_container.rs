//! A block-level HTML element imported from Markdown stays inside the container
//! that holds it, in the position the source put it.
//!
//! The importer read `Tag::HtmlBlock` through the catch-all arm of `start`,
//! which opens a paragraph frame. The raw block was then emitted while that
//! frame was on top of the stack, so it missed the enclosing quote or list item
//! and landed at the top of the document - ahead of the container it was
//! written inside. An attribution therefore preceded the quotation it belonged
//! to. The stray frame also closed as an empty paragraph, which the Carve
//! writer drops but an AST consumer sees.
//!
//! Every expectation below was taken from `commonmark` 0.31.2 and `marked`
//! 18.0.9, installed outside this checkout. Both readers agree on all of them:
//! the element is an unwrapped block child of the container, in document order.
//! For example, for
//!
//! ```text
//! first para
//!
//! > quoted
//! >
//! > <footer>Socrates</footer>
//!
//! last para
//! ```
//!
//! `commonmark` reports
//!
//! ```html
//! <p>first para</p>
//! <blockquote>
//! <p>quoted</p>
//! <footer>Socrates</footer>
//! </blockquote>
//! <p>last para</p>
//! ```
//!
//! Reported as `markup-carve/carve-rs#963`, alongside the same rule in
//! `markup-carve/carve-js#1045`.

use carve::{BlockNode, Document};

fn carve(markdown: &str) -> String {
    carve::markdown_to_carve(markdown)
}

/// The type names of a document's top-level children, in order. The point of
/// the sweep is placement, so the assertion has to see order, not membership.
fn top_level(doc: &Document) -> Vec<&'static str> {
    doc.children.iter().map(node_name).collect()
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

#[test]
fn a_quote_keeps_the_element_after_a_blank_quote_line() {
    assert_eq!(
        carve("first para\n\n> quoted\n>\n> <footer>Socrates</footer>\n\nlast para\n"),
        "first para\n\n> quoted\n>\n> ```=html\n> <footer>Socrates</footer>\n> ```\n\nlast para\n",
    );
}

/// The order is the whole defect, so it gets an assertion that only order can
/// satisfy: the quote came second and stayed second.
#[test]
fn the_element_does_not_move_ahead_of_the_quote_that_holds_it() {
    let doc = carve::markdown_to_ast(
        "first para\n\n> quoted\n>\n> <footer>Socrates</footer>\n\nlast para\n",
    );
    assert_eq!(top_level(&doc), ["paragraph", "block_quote", "paragraph"]);

    let BlockNode::BlockQuote(quote) = &doc.children[1] else {
        panic!("the second child is the quote: {:?}", top_level(&doc));
    };
    assert_eq!(
        quote.children.iter().map(node_name).collect::<Vec<_>>(),
        ["paragraph", "raw_block"],
    );
}

#[test]
fn a_quote_keeps_an_element_that_interrupts_quoted_prose() {
    assert_eq!(
        carve("> quoted\n> <footer>Socrates</footer>\n> more\n"),
        "> quoted\n>\n> ```=html\n> <footer>Socrates</footer>\n> more\n> ```\n",
    );
}

#[test]
fn a_nested_quote_keeps_the_element_at_its_own_depth() {
    assert_eq!(
        carve("> outer\n>\n> > inner\n> >\n> > <footer>x</footer>\n"),
        "> outer\n>\n> > inner\n> >\n> > ```=html\n> > <footer>x</footer>\n> > ```\n",
    );
}

#[test]
fn a_quote_keeps_a_multi_line_element_whole() {
    assert_eq!(
        carve("> quoted\n>\n> <div>\n> line one\n> line two\n> </div>\n\nafter\n"),
        "> quoted\n>\n> ```=html\n> <div>\n> line one\n> line two\n> </div>\n> ```\n\nafter\n",
    );
}

#[test]
fn a_quote_keeps_an_html_comment() {
    assert_eq!(
        carve("> quoted\n>\n> <!-- note -->\n\nafter\n"),
        "> quoted\n>\n> ```=html\n> <!-- note -->\n> ```\n\nafter\n",
    );
}

#[test]
fn a_quote_keeps_a_script_element() {
    assert_eq!(
        carve("> quoted\n>\n> <script>\n> var a = 1;\n> </script>\n\nafter\n"),
        "> quoted\n>\n> ```=html\n> <script>\n> var a = 1;\n> </script>\n> ```\n\nafter\n",
    );
}

#[test]
fn a_quote_holding_only_the_element_still_holds_it() {
    assert_eq!(
        carve("> <footer>x</footer>\n\nafter\n"),
        "> ```=html\n> <footer>x</footer>\n> ```\n\nafter\n",
    );
}

#[test]
fn a_list_item_keeps_the_element_after_a_blank() {
    assert_eq!(
        carve("- item\n\n  <footer>x</footer>\n\nafter\n"),
        "- item\n\n  ```=html\n  <footer>x</footer>\n  ```\n\nafter\n",
    );
}

/// The item is tight here, so the fence follows the text with no blank line -
/// which is the shape both readers report, `<li>item` immediately followed by
/// the element.
#[test]
fn a_list_item_keeps_the_element_on_a_continuation_line() {
    assert_eq!(
        carve("- item\n  <footer>x</footer>\n\nafter\n"),
        "- item\n  ```=html\n  <footer>x</footer>\n  ```\n\nafter\n",
    );
}

#[test]
fn an_ordered_item_keeps_the_element() {
    assert_eq!(
        carve("1. item\n\n   <footer>x</footer>\n\nafter\n"),
        "1. item\n\n   ```=html\n   <footer>x</footer>\n   ```\n\nafter\n",
    );
}

#[test]
fn a_nested_item_keeps_the_element_at_its_content_column() {
    assert_eq!(
        carve("- outer\n\n  - inner\n\n    <footer>x</footer>\n\nafter\n"),
        "- outer\n\n  - inner\n\n    ```=html\n    <footer>x</footer>\n    ```\n\nafter\n",
    );
}

#[test]
fn an_item_inside_a_quote_keeps_the_element() {
    assert_eq!(
        carve("> - item\n>\n>   <footer>x</footer>\n\nafter\n"),
        "> - item\n>\n>   ```=html\n>   <footer>x</footer>\n>   ```\n\nafter\n",
    );
}

#[test]
fn a_quote_inside_an_item_keeps_the_element() {
    assert_eq!(
        carve("- item\n\n  > quoted\n  >\n  > <footer>x</footer>\n\nafter\n"),
        "- item\n\n  > quoted\n  >\n  > ```=html\n  > <footer>x</footer>\n  > ```\n\nafter\n",
    );
}

#[test]
fn a_list_item_keeps_an_html_comment() {
    assert_eq!(
        carve("- item\n\n  <!-- note -->\n\nafter\n"),
        "- item\n\n  ```=html\n  <!-- note -->\n  ```\n\nafter\n",
    );
}

#[test]
fn an_item_holding_only_the_element_still_holds_it() {
    assert_eq!(
        carve("- <footer>x</footer>\n\nafter\n"),
        "- ```=html\n  <footer>x</footer>\n  ```\n\nafter\n",
    );
}

/// Two elements in one item keep their order relative to each other and to the
/// item's prose.
#[test]
fn two_elements_in_one_item_keep_their_order() {
    let doc = carve::markdown_to_ast("- item\n\n  <footer>a</footer>\n\n  <footer>b</footer>\n");
    let BlockNode::List(list) = &doc.children[0] else {
        panic!("the only child is the list: {:?}", top_level(&doc));
    };
    let blocks = &list.items[0].children;
    assert_eq!(
        blocks.iter().map(node_name).collect::<Vec<_>>(),
        ["paragraph", "raw_block", "raw_block"],
    );
    let (BlockNode::RawBlock(first), BlockNode::RawBlock(second)) = (&blocks[1], &blocks[2]) else {
        panic!("both raw blocks are in the item");
    };
    assert_eq!(first.content, "<footer>a</footer>");
    assert_eq!(second.content, "<footer>b</footer>");
}

/// A footnote definition is a container the same way, and the element inside it
/// used to escape all the way to the top of the document. Neither reference
/// reader implements footnotes, so the reading here is the parser's own event
/// stream, which places the element inside the definition.
#[test]
fn a_footnote_definition_keeps_the_element() {
    let doc = carve::markdown_to_ast("[^1]: note\n\n    <footer>x</footer>\n\nafter\n");
    assert_eq!(top_level(&doc), ["paragraph"]);
    assert_eq!(
        doc.footnote_defs["1"]
            .iter()
            .map(node_name)
            .collect::<Vec<_>>(),
        ["paragraph", "raw_block"],
    );
}

/// At top level the element was already in the right place, but the stray frame
/// closed as an empty paragraph that an AST consumer sees.
#[test]
fn a_top_level_element_leaves_no_empty_paragraph_behind() {
    let doc = carve::markdown_to_ast("before\n\n<footer>x</footer>\n\nafter\n");
    assert_eq!(top_level(&doc), ["paragraph", "raw_block", "paragraph"]);
}

#[test]
fn a_quote_leaves_no_empty_paragraph_behind() {
    let doc = carve::markdown_to_ast("> quoted\n>\n> <footer>x</footer>\n");
    let BlockNode::BlockQuote(quote) = &doc.children[0] else {
        panic!("the only child is the quote: {:?}", top_level(&doc));
    };
    assert_eq!(
        quote.children.iter().map(node_name).collect::<Vec<_>>(),
        ["paragraph", "raw_block"],
    );
}

/// A condition 7 block runs to the next blank line rather than stopping at its
/// opening line, and a condition 6 block runs past its closing tag to the next
/// blank line. Both readers agree, and both were already right here - this pins
/// them so the container work cannot quietly narrow them.
#[test]
fn a_condition_7_block_runs_to_the_blank_line() {
    assert_eq!(
        carve("before\n\n<custom-tag attr=\"v\">\nbody\n</custom-tag>\n\nafter\n"),
        "before\n\n```=html\n<custom-tag attr=\"v\">\nbody\n</custom-tag>\n```\n\nafter\n",
    );
}

#[test]
fn a_condition_6_block_runs_past_its_closing_tag() {
    assert_eq!(
        carve("before\n\n<div>\ninside\n</div>\ntrailing\n\nafter\n"),
        "before\n\n```=html\n<div>\ninside\n</div>\ntrailing\n```\n\nafter\n",
    );
}

#[test]
fn an_opener_after_prose_is_its_own_block() {
    assert_eq!(
        carve("prose line\n<footer>x</footer>\n\nafter\n"),
        "prose line\n\n```=html\n<footer>x</footer>\n```\n\nafter\n",
    );
}

/// CONTROL: a genuinely inline element in a container stays inline and stays
/// put. This is what proves the fix discriminates rather than treating every
/// tag as a block.
#[test]
fn an_inline_span_in_a_quote_stays_inline() {
    assert_eq!(carve("> a <span>b</span> c\n"), "> a <span>b</span> c\n");
}

/// A tight list item is asserted on the tree rather than on the written Carve,
/// because the writer splits a tight item's inline run across lines for reasons
/// that have nothing to do with HTML - the same happens to `- a *b* c`. What
/// this control has to show is that the span is still inline text and no raw
/// block was opened for it.
#[test]
fn an_inline_span_in_a_list_item_stays_inline() {
    let doc = carve::markdown_to_ast("- a <span>b</span> c\n");
    let BlockNode::List(list) = &doc.children[0] else {
        panic!("the only child is the list: {:?}", top_level(&doc));
    };
    let kinds = list.items[0]
        .children
        .iter()
        .map(node_name)
        .collect::<Vec<_>>();
    assert!(
        !kinds.contains(&"raw_block"),
        "an inline span opened a raw block: {kinds:?}",
    );
    assert_eq!(
        carve::to_json(&doc).matches("<span>").count(),
        1,
        "the span survives as text exactly once",
    );
}

#[test]
fn an_inline_span_at_top_level_stays_inline() {
    assert_eq!(carve("a <span>b</span> c\n"), "a <span>b</span> c\n");
}

/// CONTROL: a table cell holds inline content in CommonMark too, so nothing
/// about it changes.
#[test]
fn an_inline_span_in_a_table_cell_stays_inline() {
    assert_eq!(
        carve("| h |\n|---|\n| a <span>b</span> c |\n"),
        "|=h|\n| a <span>b</span> c |\n",
    );
}

/// CONTROL: an ordinary container with no HTML in it is untouched.
#[test]
fn a_container_without_html_is_unchanged() {
    assert_eq!(
        carve("> quoted\n>\n> more\n\n- item\n\n  second\n"),
        "> quoted\n>\n> more\n\n- item\n\n  second\n",
    );
}
