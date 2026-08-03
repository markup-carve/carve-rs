//! PART 12 section 7: a definition is a child of the DOCUMENT even when it was
//! authored inside a container, because its scope is the document wherever it
//! sits. A footnote definition already worked that way here; an abbreviation
//! definition did not (carve-php#631, spec carve#518).
//!
//! Two separate defects met in this one place, and the tests below keep them
//! apart:
//!
//!   - the SHAPE: the definition stayed nested, so `fmt` and the serialized
//!     tree disagreed with carve-php about where it lives;
//!   - the RENDERING: `apply_abbreviations` collects definitions from
//!     `doc.children` alone, so one written inside a container was never
//!     collected, AND `apply_abbreviations_block` had no arm for a `:::` div,
//!     so an abbreviation never expanded inside one even when the definition
//!     sat at the top level where collection was never in doubt. carve-js
//!     expands in both cases.

use carve::ast::BlockNode;
use carve::{parse, to_carve, to_html};

fn types(blocks: &[BlockNode]) -> Vec<&'static str> {
    blocks
        .iter()
        .map(|b| match b {
            BlockNode::Div(_) => "div",
            BlockNode::List(_) => "list",
            BlockNode::BlockQuote(_) => "block_quote",
            BlockNode::Paragraph(_) => "paragraph",
            BlockNode::AbbreviationDef(_) => "abbreviation_def",
            _ => "other",
        })
        .collect()
}

/// `parse` runs the HTML pipeline, which consumes the definitions after
/// collecting them, so the shape is checked through the writer instead - it
/// reads the same tree.
fn formatted(source: &str) -> String {
    to_carve(source)
}

#[test]
fn a_definition_in_a_div_moves_to_the_document() {
    let out = formatted(":::\n*[HTML]: HyperText\n\nbody\n:::\n");
    assert_eq!(out, ":::\nbody\n:::\n\n*[HTML]: HyperText\n");
}

#[test]
fn a_definition_in_a_list_item_moves_to_the_document() {
    let out = formatted("- a\n\n  *[X]: ex\n");
    assert!(
        out.trim_end().ends_with("*[X]: ex"),
        "definition should be written at document level, got:\n{out}"
    );
}

#[test]
fn a_definition_in_a_block_quote_moves_to_the_document() {
    let out = formatted("> a\n>\n> *[X]: ex\n");
    assert!(
        out.trim_end().ends_with("*[X]: ex"),
        "definition should be written at document level, got:\n{out}"
    );
}

#[test]
fn a_definition_the_author_wrote_at_document_level_stays_put() {
    let out = formatted("*[HTML]: HyperText\n\nbody\n");
    assert_eq!(out, "*[HTML]: HyperText\n\nbody\n");
}

#[test]
fn a_container_holding_only_a_definition_is_left_empty_not_removed() {
    // Section 7 says the container is left empty by the collection, which is
    // already what a footnote definition does.
    let out = formatted(":::\n*[X]: ex\n:::\n");
    assert!(
        out.starts_with(":::"),
        "the div should survive the hoist, got:\n{out}"
    );
}

#[test]
fn the_document_still_parses_the_definition_out_of_the_container() {
    // The tree the HTML pipeline sees has no definition left in the div: it was
    // hoisted, then consumed by `apply_abbreviations`.
    let doc = parse(":::\n*[HTML]: HyperText\n\nbody\n:::\n");
    assert_eq!(types(&doc.children), vec!["div"]);
}

#[test]
fn an_abbreviation_defined_in_a_container_expands() {
    // The rendering half. This was plain text before: the definition was never
    // collected, because collection only ever looked at `doc.children`.
    let html = to_html(":::\n*[HTML]: HyperText\n\nHTML rocks\n:::\n");
    assert!(
        html.contains("<abbr title=\"HyperText\">HTML</abbr>"),
        "expected the abbreviation to expand, got:\n{html}"
    );
}

#[test]
fn an_abbreviation_expands_inside_a_div_from_a_top_level_definition() {
    // The other half, and a defect on its own: `apply_abbreviations_block` had
    // no `Div` arm, so this failed even though the definition was collected.
    let html = to_html("*[HTML]: HyperText\n\n:::\nHTML rocks\n:::\n");
    assert!(
        html.contains("<abbr title=\"HyperText\">HTML</abbr>"),
        "expected the abbreviation to expand inside the div, got:\n{html}"
    );
}

#[test]
fn an_abbreviation_expands_inside_a_definition_list() {
    let html = to_html("*[HTML]: HyperText\n\n:: term\n:  HTML rocks\n");
    assert!(
        html.contains("<abbr title=\"HyperText\">HTML</abbr>"),
        "expected the abbreviation to expand in the description, got:\n{html}"
    );
}

#[test]
fn the_abbreviation_still_expands_outside_the_container_it_was_defined_in() {
    // The scope was always the document; hoisting is what makes the tree say so.
    let html = to_html(":::\n*[HTML]: HyperText\n:::\n\nHTML rocks\n");
    assert!(
        html.contains("<abbr title=\"HyperText\">HTML</abbr>"),
        "expected the abbreviation to expand after the div, got:\n{html}"
    );
}
