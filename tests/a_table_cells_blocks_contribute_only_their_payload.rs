//! PART 12 §27 (`CARVE-P12-049`), as ruled on markup-carve/carve#2389 and
//! markup-carve/carve#2390: what a block inside `table_cell.blocks` contributes
//! to the one line the flattening targets write.
//!
//! §27 says a cell's block contributes "its payload". Two kinds of block carry
//! something that is neither payload nor inline content, and both were ruled on
//! 2026-09-26:
//!
//! - a code block's quoted header and bracketed label are omitted (#2389). At
//!   document level plain and ANSI print each as its own line, so omitting them
//!   only inside a cell makes those targets slightly inconsistent with
//!   themselves; the clause was chosen over that consistency, because a cell is
//!   one line.
//! - a `raw_block` and an `abbreviation_def` contribute NOTHING (#2390). §27's
//!   sentence still reads "a code block or a raw block contributes its payload
//!   ... a raw block's as escaped text rather than live markup", which the
//!   ruling reverses; the spec sentence is owed, and this engine follows the
//!   ruling. carve#589 refused to drop a definition on a line target, but
//!   nothing an author wrote is lost here: Carve 0.1 source has no spelling for
//!   a block inside a cell at all.
//!
//! The documents are AST JSON for that reason - there is no source to parse.
//! markup-carve/carve-rs#2007 reported that a block-bearing cell could not be
//! ingested at all; it was read against a stale checkout, and the ingest half is
//! asserted here so the claim stays measured rather than remembered.

use carve::{from_json, render_ansi, render_html, render_markdown, render_plain_text};

fn one_cell(blocks: &str) -> String {
    format!(
        r#"{{"type":"document","srcByteLength":0,"children":[
             {{"type":"table","rows":[
               {{"type":"table_row","cells":[
                 {{"type":"table_cell","header":false,"blocks":[{blocks}]}}
               ]}}
             ]}}
           ]}}"#
    )
}

/// The four targets for one document, so a divergence between them is visible in
/// the failure rather than in the next test.
fn targets(blocks: &str) -> (String, String, String, String) {
    let doc = from_json(&one_cell(blocks)).expect("a block-bearing cell decodes");
    (
        render_markdown(&doc).expect("markdown"),
        render_plain_text(&doc).expect("plain"),
        render_ansi(&doc).expect("ansi"),
        render_html(&doc).expect("html"),
    )
}

/// carve-rs#2007's own payload and expectation. A cell carrying `blocks` and no
/// `children` decodes, and the code block's newline becomes one space.
#[test]
fn a_cell_carrying_blocks_and_no_children_is_ingested_and_flattened() {
    let (md, plain, ansi, _) =
        targets(r#"{"type":"code_block","content":"first\nsecond\n","lang":"js"}"#);
    assert!(md.contains("| first second |"), "{md}");
    assert!(plain.contains("first second"), "{plain}");
    assert!(ansi.contains("first second"), "{ansi}");
}

/// A cell carrying BOTH is refused, and a cell carrying NEITHER is too: §27 says
/// exactly one. Without this the test above could pass on a decoder that simply
/// ignored `children`.
#[test]
fn a_cell_carrying_both_fields_or_neither_is_refused() {
    let both = r#"{"type":"document","srcByteLength":0,"children":[
        {"type":"table","rows":[{"type":"table_row","cells":[
          {"type":"table_cell","header":false,"children":[{"type":"text","value":"a"}],
           "blocks":[{"type":"paragraph","children":[{"type":"text","value":"b"}]}]}
        ]}]}]}"#;
    assert!(from_json(both).is_err(), "a cell carried both fields");
    let neither = r#"{"type":"document","srcByteLength":0,"children":[
        {"type":"table","rows":[{"type":"table_row","cells":[
          {"type":"table_cell","header":false}
        ]}]}]}"#;
    assert!(from_json(neither).is_err(), "a cell carried neither field");
}

/// carve#2389. The header and the label are omitted on every flattening target.
#[test]
fn a_code_blocks_header_and_label_do_not_reach_the_cell() {
    let (md, plain, ansi, _) = targets(
        r#"{"type":"code_block","content":"x = 1\n","lang":"php",
            "header":"src/Auth.php","label":"NPM"}"#,
    );
    for (target, out) in [("markdown", &md), ("plain", &plain), ("ansi", &ansi)] {
        assert!(
            !out.contains("src/Auth.php"),
            "{target} wrote the header into the cell: {out}"
        );
        assert!(
            !out.contains("NPM"),
            "{target} wrote the label into the cell: {out}"
        );
    }
    assert!(md.contains("| x = 1 |"), "{md}");
}

/// carve#2390, the raw block half. It contributes nothing, so the paragraph
/// beside it is the whole cell and takes no separator for a side that is not
/// there - which is PART 11 §1b's rule that an empty block is not a side.
#[test]
fn a_raw_block_contributes_nothing_to_the_cell() {
    let (md, plain, ansi, _) = targets(
        r#"{"type":"paragraph","children":[{"type":"text","value":"a"}]},
           {"type":"raw_block","format":"html","content":"<b>gone</b>\n"}"#,
    );
    for (target, out) in [("markdown", &md), ("plain", &plain), ("ansi", &ansi)] {
        assert!(
            !out.contains("gone"),
            "{target} contributed the raw block's payload: {out}"
        );
        assert!(!out.contains("<b>"), "{target} leaked the spelling: {out}");
    }
    assert!(md.contains("| a |"), "{md}");
    assert!(
        !md.contains("| a  |"),
        "a separator for an absent side: {md}"
    );
}

/// carve#2390, the abbreviation half. Neither the abbreviation nor its expansion
/// reaches the cell.
#[test]
fn an_abbreviation_definition_contributes_nothing_to_the_cell() {
    let (md, plain, ansi, _) = targets(
        r#"{"type":"paragraph","children":[{"type":"text","value":"a"}]},
           {"type":"abbreviation_def","abbr":"HTTP","expansion":"HyperText Transfer Protocol"}"#,
    );
    for (target, out) in [("markdown", &md), ("plain", &plain), ("ansi", &ansi)] {
        assert!(!out.contains("HTTP"), "{target} wrote the abbr: {out}");
        assert!(
            !out.contains("HyperText"),
            "{target} wrote the expansion: {out}"
        );
    }
    assert!(md.contains("| a |"), "{md}");
}

/// A raw block ALONE leaves the cell empty rather than falling back to its
/// payload. Paired with the test above, this is the control that separates
/// "contributes nothing" from "is dropped when something else contributes".
#[test]
fn a_cell_holding_only_a_raw_block_is_empty() {
    let (md, _, _, _) =
        targets(r#"{"type":"raw_block","format":"html","content":"<b>gone</b>\n"}"#);
    assert!(!md.contains("gone"), "{md}");
    assert!(md.contains("|  |"), "{md}");
}

/// HTML is OUTSIDE both rulings. §27 puts a cell's blocks in the `<td>` as flow
/// content under the ordinary block rules, and the rulings speak to the sentence
/// about what a block CONTRIBUTES to a flattened line. So the code block keeps
/// its header on this target, and the raw block keeps its payload.
#[test]
fn the_html_target_still_renders_the_blocks_in_the_cell() {
    let (_, _, _, html) = targets(
        r#"{"type":"code_block","content":"x = 1\n","lang":"php","header":"src/Auth.php"},
           {"type":"raw_block","format":"html","content":"<b>kept</b>\n"}"#,
    );
    assert!(html.contains("src/Auth.php"), "{html}");
    assert!(html.contains("<b>kept</b>"), "{html}");
}

#[test]
fn code_spaces_and_empty_lines_survive_the_flatten() {
    let (md, plain, ansi, _) = targets(r#"{"type":"code_block","content":"a  b\n\nc"}"#);
    for out in [md, plain, ansi] {
        assert!(out.contains("a  b  c"), "{out}");
    }
}
