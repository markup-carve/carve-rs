//! A line block is its own AST node (`LineBlock`), not a div carrying a
//! `.line-block` class.
//!
//! The class alone cannot express the difference: inside a `::: |` fence every
//! newline is a hard break, while a plain div an author gave that class keeps
//! soft breaks. With only the class to go on the writer could not tell which one
//! to emit, so it emitted the generic form and a formatted line block re-parsed
//! as an ordinary div - `parse(fmt(x)) == parse(x)` did not hold (carve issue
//! 359). The spec's profiles.md block vocabulary lists `line_block` for the same
//! reason: a profile denying it has to be able to name it.

use carve::ast::BlockNode;

const SOURCE: &str = "::: |\nRoses are red,\n  Violets are blue.\n:::\n";

#[test]
fn parses_to_its_own_node_type() {
    let doc = carve::parse(SOURCE);
    assert!(matches!(
        doc.children.first(),
        Some(BlockNode::LineBlock(_))
    ));
}

#[test]
fn still_renders_as_a_div_carrying_the_line_block_class() {
    // The class is part of the output contract, not of the AST.
    assert!(carve::to_html(SOURCE).contains("<div class=\"line-block\">"));
}

#[test]
fn keeps_author_attributes_alongside_the_structural_class() {
    // The structural class trails the author's attributes, matching carve-php
    // and carve-js.
    assert!(carve::to_html(&format!("{{#verse}}\n{SOURCE}"))
        .contains("<div id=\"verse\" class=\"line-block\">"));
    assert!(carve::to_html(&format!("{{.foo #v}}\n{SOURCE}"))
        .contains("<div class=\"foo line-block\" id=\"v\">"));
}

#[test]
fn round_trips_through_the_writer_byte_for_byte() {
    assert_eq!(carve::to_carve(SOURCE), SOURCE);
}

#[test]
fn preserves_the_indent_as_spaces_not_as_a_literal_nbsp() {
    // The parser records the indent with the U+E000 placeholder, which the
    // writer used to resolve to a real nbsp - and a real nbsp re-parses as
    // literal text rather than as indentation.
    let out = carve::to_carve(SOURCE);
    assert!(out.contains("\n  Violets"));
    assert!(!out.contains('\u{00a0}'));
}

#[test]
fn is_idempotent() {
    let once = carve::to_carve(SOURCE);
    assert_eq!(carve::to_carve(&once), once);
}

#[test]
fn a_plain_div_carrying_the_class_stays_a_div() {
    let doc = carve::parse("{.line-block}\n:::\nRoses are red,\n:::\n");
    assert!(matches!(doc.children.first(), Some(BlockNode::Div(_))));
}
