//! Paragraph ends include an unplaced last text child.
//! Unchanged text retains its own source span (markup-carve/carve#2175).

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
/// line at 6..8, the terminator ending it at 8..9, the merged text at 9..16.
const TABBED_LAST: &str = "::: |\n%%\ntab\tgap\n:::\n";

#[test]
fn a_stanza_paragraph_ends_on_its_own_last_line() {
    // It used to end at 9 - one past the terminator above the tab-bearing line,
    // so the line the paragraph is FOR sat outside it.
    assert_eq!(nth(TABBED_LAST, "paragraph", 0), (6, 16));
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
    let source = "::: |\n%%\ntab\tgap\n:::\n";
    assert_eq!(placed(source, "text", 0), None);
    assert_eq!(nth(source, "paragraph", 0), (6, 16));
}

#[test]
fn a_stanza_with_no_comment_line_is_the_same_case() {
    // The arrangement without a `%%` line at all: two verse lines, a tab on the
    // second. `tab<TAB>gap` runs 8..15, and the paragraph used to end at 8, where
    // the terminator after `a` does.
    assert_eq!(nth("::: |\na\ntab\tgap\n:::\n", "paragraph", 0), (6, 15));
}

#[test]
fn it_holds_at_every_depth() {
    // A stanza inside a quote and inside an item. Each is a separate walk in
    // this engine, and the derivation this fixes is reached from all of them.
    assert_eq!(
        nth("> ::: |\n> %%\n> tab\tgap\n> :::\n", "paragraph", 0),
        (10, 22)
    );
    assert_eq!(
        nth("- ::: |\n  %%\n  tab\tgap\n  :::\n", "paragraph", 0),
        (10, 22)
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
    let footnote = "[^1]: ::: |\n    a\n    tab\tgap\n\nx[^1]\n";
    let (start, end) = nth(footnote, "paragraph", 1);
    let (text_start, text_end) = nth(footnote, "text", 1);

    assert_eq!((start, end), (14, 29));
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
    assert_eq!(nth("::: |\ntab\tgap\n%%\n:::\n", "paragraph", 0), (6, 16));
    assert_eq!(nth("::: |\na\nb\n:::\n", "paragraph", 0), (6, 9));
}
