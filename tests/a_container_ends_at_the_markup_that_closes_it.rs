//! A container ends at the MARKUP THAT CLOSES IT, whether or not its last child
//! is placed (markup-carve/carve#1551).
//!
//! The MIRROR of `a_container_starts_at_its_opening_markup.rs`, and the last
//! arrangement the extent rules did not name. A line block stanza rewrites the
//! whitespace it preserves to a sentinel, one per column, so every character
//! keeps its own offset - except where the line holds a TAB. A tab expands to
//! up to four columns from one source character, so the stanza's text is
//! REASSEMBLED rather than sliced, and PART 12 section 4 has this engine
//! publish no position for it. That was ruled explicitly and stays.
//!
//! Put the tab-bearing line LAST and that text is the paragraph's last child.
//! This engine then ended the paragraph at the last child that DID carry a
//! position - the `hard_break` closing the line above - which is offset 9 on
//! the document below, one past the terminator that break owns. So the span
//! ended immediately after a line terminator, which section 4 excludes by name,
//! and the stanza's own last line fell outside the paragraph holding it. carve-js
//! and carve-php ended it at 12, where that line ends.
//!
//! READ AS TWO STATEMENTS ABOUT MARKUP the two halves are symmetric: a
//! container starts at the markup that opens it and ends at the markup that
//! closes it. "Ends at its last placed child" (markup-carve/carve#1522) is the
//! case for a container whose closer is IMPLICIT, where the last child's end is
//! what it has instead of a closer - so this locates that ruling rather than
//! overturning it, and `a_container_ends_at_its_last_placed_child.rs` next door
//! still holds every case it pinned.
//!
//! NOT SEEN BY THE THREE-WAY SPAN PANEL, for the same reason as the start side
//! and one more: no corpus document held this arrangement, AND the spec's
//! `checkStopsAtChildren` skipped every container holding an unplaced child, so
//! the check enforcing markup-carve/carve#1522 declined the one arrangement
//! that ruling did not reach. Corpus 402 and the un-skip land with the clause.

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

/// Corpus 402, and the start-side document's two stanza lines swapped: the `%%`
/// line at 6..8, the terminator ending it at 8..9, the tab-bearing line 9..12.
const TABBED_LAST: &str = "::: |\n%%\na\tb\n:::\n";

#[test]
fn a_stanza_paragraph_ends_on_its_own_last_line() {
    // It used to end at 9 - one past the terminator above the tab-bearing line,
    // so the line the paragraph is FOR sat outside it.
    assert_eq!(nth(TABBED_LAST, "paragraph", 0), (6, 12));
}

#[test]
fn the_span_does_not_end_immediately_after_a_line_terminator() {
    // Stated as the property rather than as the offset, because it is the
    // property section 4 excludes by name and the reason 9 was never a matter
    // of taste.
    let (_, end) = nth(TABBED_LAST, "paragraph", 0);
    let previous = TABBED_LAST
        .chars()
        .nth(end as usize - 1)
        .expect("the span ends inside the document");
    assert!(
        previous != '\n' && previous != '\r',
        "the paragraph ends at {end}, one past a line terminator",
    );
}

#[test]
fn the_reassembled_text_still_carries_no_position() {
    // Ruled explicitly alongside the above, and pinned here so a later change
    // cannot make this file pass by fabricating an offset for the text instead
    // of by widening the paragraph.
    assert_eq!(placed(TABBED_LAST, "text", 0), None);
}

#[test]
fn a_stanza_with_no_comment_line_is_the_same_case() {
    // The arrangement without a `%%` line at all: two verse lines, a tab on the
    // second. `b<TAB>c` runs 8..11, and the paragraph used to end at 8, where
    // the terminator after `a` does.
    assert_eq!(nth("::: |\na\nb\tc\n:::\n", "paragraph", 0), (6, 11));
}

#[test]
fn it_holds_at_every_depth() {
    // A stanza inside a quote and inside an item. Each is a separate walk in
    // this engine, and the derivation this fixes is reached from all of them.
    assert_eq!(
        nth("> ::: |\n> %%\n> a\tb\n> :::\n", "paragraph", 0),
        (10, 18)
    );
    assert_eq!(
        nth("- ::: |\n  %%\n  a\tb\n  :::\n", "paragraph", 0),
        (10, 18)
    );
}

#[test]
fn the_start_is_not_given_up_with_the_end() {
    // A footnote body is a separate block list, and there the block layer's
    // extent and the inline offsets DISAGREE - a question still open across the
    // engines. So the two ends have to be taken separately: a first draft that
    // required both before touching either handed the start back to the block
    // layer too, and the stanza's first `text` at 14..17 then sat OUTSIDE its
    // own paragraph, which is the containment defect these rules exist to
    // prevent, arrived at from the third side.
    let footnote = "[^1]: ::: |\n    a\n    b\tc\n\nx[^1]\n";
    let (start, end) = nth(footnote, "paragraph", 1);
    let (text_start, text_end) = nth(footnote, "text", 1);

    assert_eq!((start, end), (14, 25));
    assert!(
        start <= text_start && end >= text_end,
        "the stanza's first text at {text_start}..{text_end} sits outside its paragraph at \
         {start}..{end}",
    );
}

#[test]
fn a_placed_last_child_still_narrows_the_end() {
    // The rule has to be the reason the spans moved, not the documents: where
    // the last child IS placed, the paragraph still ends where it did. The
    // second is the start-side document, whose last child is the `comment` an
    // emptied `%%` line leaves - placed, so its end is still the paragraph's.
    assert_eq!(nth("a\n", "paragraph", 0), (0, 1));
    assert_eq!(nth("::: |\na\tb\n%%\n:::\n", "paragraph", 0), (6, 12));
    assert_eq!(nth("::: |\na\nb\n:::\n", "paragraph", 0), (6, 9));
}
