//! A container starts at the MARKUP THAT OPENS IT, whether or not its first
//! child is placed (markup-carve/carve-rs#1247).
//!
//! A line block stanza rewrites the whitespace it preserves to a sentinel, one
//! per column, so every character keeps its own offset - except where the line
//! holds a TAB. A tab expands to up to four columns from one source character,
//! so the stanza's text is REASSEMBLED rather than sliced, and PART 12 section
//! 4 has all three engines publish no position for it. That was ruled
//! explicitly and stays: a tab's display width is not a source length, so any
//! offset inside that text would be wrong in a way a consumer cannot detect.
//!
//! This engine then started the stanza's paragraph at the first child that DID
//! carry a position, which dropped the tab-bearing line out of the paragraph's
//! extent entirely - and left the `hard_break` that ends that line OUTSIDE the
//! paragraph holding it. `docs/ast-json.md` states that a parent's span
//! contains every child's, and the spec repository's `checkContainment` (in
//! scripts/spec/ast-positions.mjs) checks it in a pass of its own, which named
//! this engine and no other.
//!
//! THE START AND END RULES ARE NOT SYMMETRIC, and this file exists next to
//! `a_container_ends_at_its_last_placed_child.rs` to say so. That one asks
//! where a container's CONTENT stops, so its last placed child is the right
//! boundary. This one asks where the CONSTRUCT begins, and a construct begins
//! at its own markup - an unplaced child says nothing about where the author
//! wrote it.
//!
//! NOT SEEN BY THE THREE-WAY SPAN PANEL, which is why an illegal tree sat here:
//! no corpus document put a tab in a line block stanza that also holds a
//! comment line, so the panel had nothing to compare. The pair is in the corpus
//! now.

use serde_json::Value;

fn ast(source: &str) -> Value {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    serde_json::from_str(&carve::to_json(&carve::parse_with_options(
        source, &options,
    )))
    .expect("the serializer emits JSON")
}

fn spans(node: &Value, out: &mut Vec<(String, Option<Value>)>) {
    match node {
        Value::Array(items) => items.iter().for_each(|item| spans(item, out)),
        Value::Object(fields) => {
            if let Some(Value::String(ty)) = fields.get("type") {
                out.push((ty.clone(), fields.get("pos").cloned()));
            }
            for (key, value) in fields {
                if key != "pos" {
                    spans(value, out);
                }
            }
        }
        _ => {}
    }
}

/// The `nth` node of `ty` in serialization order, as `(startOffset, endOffset)`.
fn nth(source: &str, ty: &str, nth: usize) -> (u64, u64) {
    placed(source, ty, nth)
        .unwrap_or_else(|| panic!("no {ty} #{nth} in {source:?}, or it carries no position"))
}

/// The same, but `None` where the node exists and carries no position.
fn placed(source: &str, ty: &str, nth: usize) -> Option<(u64, u64)> {
    let mut found = Vec::new();
    spans(&ast(source), &mut found);
    let pos = found
        .into_iter()
        .filter(|(node_ty, _)| node_ty == ty)
        .map(|(_, pos)| pos)
        .nth(nth)
        .unwrap_or_else(|| panic!("no {ty} #{nth} in {source:?}"))?;
    Some((
        pos["startOffset"].as_u64().unwrap(),
        pos["endOffset"].as_u64().unwrap(),
    ))
}

/// The document the ruling was written against: `a`, TAB, `b` at 6..9, the
/// terminator at 9..10, the `%%` line at 10..12.
const TABBED: &str = "::: |\na\tb\n%%\n:::\n";

#[test]
fn a_stanza_paragraph_starts_at_its_own_first_line() {
    // It used to start at 10 - the comment line, one line BELOW the source it
    // is the paragraph for.
    assert_eq!(nth(TABBED, "paragraph", 0), (6, 12));
}

#[test]
fn the_break_that_ends_the_tab_bearing_line_is_inside_that_paragraph() {
    let (para_start, para_end) = nth(TABBED, "paragraph", 0);
    let (break_start, break_end) = nth(TABBED, "hard_break", 0);

    assert_eq!((break_start, break_end), (9, 10));
    assert!(
        break_start >= para_start && break_end <= para_end,
        "the break at {break_start}..{break_end} sits outside its paragraph at \
         {para_start}..{para_end}, which PART 12 containment refuses",
    );
}

#[test]
fn the_reassembled_text_still_carries_no_position() {
    // Ruled explicitly alongside the above, and pinned here so a later change
    // cannot make this file pass by fabricating an offset for the text instead.
    assert_eq!(placed(TABBED, "text", 0), None);
}

#[test]
fn the_comment_the_block_layer_emptied_keeps_its_own_line() {
    assert_eq!(nth(TABBED, "comment", 0), (10, 12));
}

#[test]
fn it_holds_at_every_depth() {
    // A stanza inside a quote, inside an item, and inside a footnote body. Each
    // is a separate walk in this engine, and the defect reached all of them.
    let quoted = "> ::: |\n> a\tb\n> %%\n> :::\n";
    assert_eq!(nth(quoted, "paragraph", 0), (10, 18));

    let item = "- ::: |\n  a\tb\n  %%\n  :::\n";
    assert_eq!(nth(item, "paragraph", 0), (10, 18));

    // A footnote body is a separate block list, so it is a separate walk again.
    // Asserted as the CONTAINMENT relation rather than as a literal offset:
    // whether a footnote's own indent sits inside the stanza's span is a
    // different question, still open across the engines, and this rule does not
    // answer it.
    let footnote = "[^1]: ::: |\n    a\tb\n    c\td\n    e\n\nx[^1]\n";
    let (start, end) = nth(footnote, "paragraph", 1);
    let (break_start, break_end) = nth(footnote, "hard_break", 0);
    assert!(
        start <= break_start && end >= break_end,
        "the footnote stanza's paragraph at {start}..{end} does not hold its first break at \
         {break_start}..{break_end}",
    );
}

#[test]
fn a_placed_first_child_still_narrows_the_start() {
    // The rule has to be the reason the spans moved, not the documents: where
    // the first child IS placed, the paragraph still starts where it does.
    assert_eq!(nth("a\n", "paragraph", 0), (0, 1));
    assert_eq!(nth("::: |\na\nb\n:::\n", "paragraph", 0), (6, 9));
}
