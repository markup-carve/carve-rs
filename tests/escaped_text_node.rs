//! An escaped character is its own AST node.
//!
//! The backslash carries intent the literal character does not: `\-\-` was
//! written precisely so a downstream processor with smart punctuation on would
//! NOT read an en dash. Flattening it into text lost that, and the Markdown
//! target emitted the trigger bare where carve-php reproduced the escape
//! (carve issue 350). The inline vocabulary in the spec's profiles.md lists
//! `escaped_text` for the same reason.

use carve::ast::{BlockNode, InlineNode};

const SOURCE: &str = "A \\\" B \\-\\- C \\.\\.\\. D \\* E \\_ F\n";

#[test]
fn parses_to_its_own_node_type() {
    let doc = carve::parse("a\\-b\n");
    let BlockNode::Paragraph(p) = &doc.children[0] else {
        panic!("expected a paragraph");
    };
    assert!(matches!(
        p.children.as_slice(),
        [
            InlineNode::Text(_),
            InlineNode::EscapedText(_),
            InlineNode::Text(_)
        ]
    ));
}

#[test]
fn renders_the_bare_character_in_html() {
    // The backslash is authoring syntax; the reader sees the character.
    assert_eq!(
        carve::to_html(SOURCE).trim(),
        "<p>A \" B -- C ... D * E _ F</p>"
    );
}

#[test]
fn reproduces_the_escape_on_the_markdown_target() {
    // PART 11 section 7 M2.
    assert_eq!(
        carve::to_markdown(SOURCE).trim(),
        "A \\\" B \\-\\- C \\.\\.\\. D \\* E \\_ F"
    );
}

#[test]
fn adds_no_backslashes_to_a_document_that_escaped_nothing() {
    // The cost of M2 falls only on documents that asked for it.
    assert_eq!(
        carve::to_markdown("A \"quoted\" phrase -- really.\n").trim(),
        "A \u{201C}quoted\u{201D} phrase \u{2013} really."
    );
}

#[test]
fn round_trips_through_the_canonical_writer() {
    assert_eq!(carve::to_carve(SOURCE), SOURCE);
}

#[test]
fn is_idempotent() {
    let once = carve::to_carve(SOURCE);
    assert_eq!(carve::to_carve(&once), once);
}

#[test]
fn keeps_quote_flanking_across_the_node_boundary() {
    // The escaped brace is a separate node but still the character before the
    // quote, and flanking reads that character (corpus 163).
    assert!(carve::to_html("\\{\"quoted\"\\}\n").contains("{\u{201C}quoted\u{201D}}"));
}

#[test]
fn leaves_an_unauthored_character_unescaped() {
    // An authored escape survives; a character next to it that needed no escape
    // does not acquire one just because the document contains escapes.
    assert_eq!(
        carve::to_carve("Literal \\-\\- must stay escaped.\n"),
        "Literal \\-\\- must stay escaped.\n"
    );
}
