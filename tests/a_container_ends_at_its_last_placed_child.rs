//! A container's span ends at its last placed child.
//!
//! A list, an item and a block quote have no closer, so their extent came from
//! the lines they CONSUMED - and a container consumes lines whose content ends
//! up somewhere else.
//!
//! A definition written at an item's content column is collected and hoisted to
//! the DOCUMENT by PART 12 section 7, so it becomes the list's sibling; the list
//! went on covering it, which put the same offsets in two nodes and left a
//! consumer resolving one offset with two answers. An attribute block that
//! attaches to nothing yields no child at all, which section 4 excludes by name.
//!
//! Nothing caught either, because all three engines did the same thing and the
//! spec repository's span panel compares the engines against EACH OTHER
//! (markup-carve/carve#1522, markup-carve/carve#1524).

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

fn spans(node: &Value, out: &mut Vec<(String, Value)>) {
    match node {
        Value::Array(items) => items.iter().for_each(|item| spans(item, out)),
        Value::Object(fields) => {
            if let (Some(Value::String(ty)), Some(pos)) = (fields.get("type"), fields.get("pos")) {
                out.push((ty.clone(), pos.clone()));
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
    let mut found = Vec::new();
    spans(&ast(source), &mut found);
    let pos = found
        .into_iter()
        .filter(|(node_ty, _)| node_ty == ty)
        .map(|(_, pos)| pos)
        .nth(nth)
        .unwrap_or_else(|| panic!("no {ty} #{nth} in {source:?}"));
    (
        pos["startOffset"].as_u64().unwrap(),
        pos["endOffset"].as_u64().unwrap(),
    )
}

#[test]
fn a_list_stops_before_the_definition_hoisted_out_of_it() {
    let source = "- a\n\n  [r]: /u\n";

    // The list used to end at 14, which is where the definition ends.
    assert_eq!(nth(source, "list", 0), (0, 3));
    assert_eq!(nth(source, "list_item", 0), (0, 3));
    // And the two no longer claim the same offsets, so offset 8 resolves to one
    // node.
    assert_eq!(nth(source, "link_reference_definition", 0), (5, 14));
}

#[test]
fn a_quote_stops_before_the_definition_hoisted_out_of_it() {
    let source = "> a\n> [r]: /u\n";

    assert_eq!(nth(source, "block_quote", 0), (0, 3));
    assert_eq!(nth(source, "link_reference_definition", 0), (4, 13));
}

#[test]
fn a_list_stops_before_an_unattached_attribute_block() {
    // `{.x}` reaches no block, so it yields no child and the list covering it
    // was covering source nothing in it owns. It used to end at 10.
    assert_eq!(nth("- a\n  {.x}\ntail\n", "list", 0), (0, 3));
}

#[test]
fn a_list_stops_before_the_terminator_that_ended_its_last_item() {
    // The blank-run half, filed separately as markup-carve/carve-rs#1232 and
    // SUBSUMED here rather than excluded: a container that must stop at its last
    // placed child cannot reach into a blank run at all.
    assert_eq!(nth("- a\n\n\n", "list", 0), (0, 3));
    assert_eq!(nth("- a\n", "list", 0), (0, 3));
}

#[test]
fn a_container_a_collected_definition_emptied_spans_its_own_markup() {
    // "Ends at its last placed child" is silent where there is none, and the
    // inner item's only content was a definition that hoisted away. Zero width
    // was rejected: it discards the marker the author typed, and is a shape
    // every consumer has to special-case (markup-carve/carve-rs#1233).
    let source = "* * [d]: u\n :\n";

    assert_eq!(nth(source, "list", 1), (2, 4));
    assert_eq!(nth(source, "list_item", 1), (2, 4));
}

#[test]
fn a_list_inside_a_footnote_body_stops_at_its_last_item_too() {
    // A FOOTNOTE BODY IS A SEPARATE BLOCK LIST. It is not under the document's
    // `children`, so every pass in the pipeline walks it separately - and the
    // first version of this rule reached only `children`, which left exactly
    // this shape ending after the terminator of its last item on one corpus
    // document out of 1352 (`206-a-nested-list-in-a-footnote-body-stays-
    // nested`). The spec repository's own checker is what found it.
    let source = "- a\n  [^f]: t\n\n    - x\n      - y\n\nz[^f]\n";

    assert_eq!(nth(source, "list", 1), (19, 32));
    assert_eq!(nth(source, "list", 2), (29, 32));
}

#[test]
fn a_container_with_children_is_unchanged() {
    // The rule has to be the reason the spans moved, not the documents.
    assert_eq!(nth("- a\n- b", "list", 0), (0, 7));
    assert_eq!(nth("> a\n> b", "block_quote", 0), (0, 7));
    // And a container that DOES have a closer still ends at it.
    assert_eq!(nth("::: n\na\n:::\n", "admonition", 0), (0, 11));
}
