//! A container's extent STOPS at the definition it hosted.
//!
//! THE DIRECTION OF THIS FILE REVERSED, and that is the subject rather than an
//! accident. A footnote or link reference definition is lifted out of the body
//! before the block parser runs, and an invisible placeholder is left on the
//! line it opened so the container still sees a non-blank line there. This
//! engine then widened every container ending on that line out to cover the
//! definition, so all three engines agreed that `- a` followed by a collected
//! definition produced a list running past its own item (carve-rs#1106).
//!
//! markup-carve/carve#1522 ruled the other way. PART 12 section 7 hoists the
//! definition to the DOCUMENT, so it is the list's sibling, not its child - and
//! two siblings covering the same offsets is exactly what section 4's
//! sibling-overlap prohibition exists to prevent. A container ends at its last
//! placed child, and the definition is not one.
//!
//! What the widening was FOR is unaffected and still asserted below: the
//! placeholder keeps the removed definition from leaving a blank line that
//! loosens the item, and the duplicate-label bookkeeping still has to leave the
//! accepted definition's own span where it was. Those tests did not move. Every
//! assertion that reversed is an extent, and each carries the value it used to
//! have.

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

fn html(source: &str) -> String {
    carve::to_html(source)
}

/// Every `(type, pos)` pair in the document, in serialization order.
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

/// The one node of `ty` that STARTS at `start_column`.
///
/// These documents nest three lists inside each other, so a bare "find the
/// list" would silently answer about whichever one comes first and pass for the
/// wrong reason. The starting column is what tells them apart, and it is also
/// the half of the span this fix never moves.
fn node_at(source: &str, ty: &str, start_column: u64) -> Value {
    let mut found = Vec::new();
    spans(&ast(source), &mut found);
    let mut matches = found
        .into_iter()
        .filter(|(node_ty, pos)| node_ty == ty && pos["startColumn"] == start_column)
        .map(|(_, pos)| pos);
    let first = matches
        .next()
        .unwrap_or_else(|| panic!("no {ty} starting at column {start_column}"));
    assert!(
        matches.next().is_none(),
        "more than one {ty} starts at column {start_column}, so the assertion below is ambiguous",
    );
    first
}

fn end(pos: &Value) -> (u64, u64, u64) {
    (
        pos["endLine"].as_u64().unwrap(),
        pos["endColumn"].as_u64().unwrap(),
        pos["endOffset"].as_u64().unwrap(),
    )
}

// ---- a multi-line definition body -------------------------------------------

/// Corpus 357-...-4. The definition sits at the item's content column with no
/// marker in front of it, and its body runs one line further - and the list
/// stops before all of it. This read `(3, 9, 22)`, the end of the definition's
/// body, which is source the list has no child for (carve#1522).
#[test]
fn a_list_stops_before_the_body_of_the_definition_it_hosted() {
    let source = "- a\n  [^f]: t\n    more\ntail\n\nx[^f]\n";

    assert_eq!(end(&node_at(source, "list", 1)), (1, 4, 3));
}

/// Corpus 359. A single blank line inside the note body keeps the note open,
/// and it makes no difference to the list: whatever the definition's body does,
/// the list ends at its own last item. This read `(4, 9, 23)`.
#[test]
fn a_blank_line_inside_the_hosted_body_makes_no_difference_to_the_list() {
    let source = "- a\n  [^f]: t\n\n    more\ntail\n\nx[^f]\n";

    assert_eq!(end(&node_at(source, "list", 1)), (1, 4, 3));
}

/// UNCHANGED BY THE REVERSAL, and the reason it is worth keeping: the item
/// always ended on line 1, because its content is `a` and nothing else. What
/// moved is the LIST, which now agrees with it - the two assertions used to
/// disagree by two lines and the disagreement was the defect.
#[test]
fn the_item_that_ended_before_the_definition_is_left_where_it_was() {
    let source = "- a\n  [^f]: t\n    more\ntail\n\nx[^f]\n";

    assert_eq!(end(&node_at(source, "list_item", 1)), (1, 4, 3));
}

/// A single-line definition is the same answer: the list ends at its item, not
/// on the line the definition opened. This read `(2, 10, 13)`.
#[test]
fn a_single_line_definition_does_not_extend_the_container_that_hosted_it() {
    let source = "- a\n  [^f]: t\ntail\n\nx[^f]\n";

    assert_eq!(end(&node_at(source, "list", 1)), (1, 4, 3));
}

// ---- an alternating container prefix ----------------------------------------

/// Corpus 360-...-2. `>` is the only structural prefix on the second line; the
/// two lists under it are reached by indentation, and the definition stands at
/// the innermost content column.
#[test]
fn a_footnote_definition_behind_an_alternating_prefix_is_outside_the_inner_list() {
    let source = "- > - - x\n  >     [^f]: note\n\nSee [^f].\n";

    // Both read `(2, 19, 28)` - the end of the definition line - while the
    // innermost item's content is `x` on line 1.
    assert_eq!(end(&node_at(source, "list", 7)), (1, 10, 9));
    assert_eq!(end(&node_at(source, "list_item", 5)), (1, 10, 9));
}

/// Corpus 360-...-4. The same shape for a LINK reference definition, which is
/// lifted out by the other prepass - both carried the same placeholder bug.
#[test]
fn a_link_definition_behind_an_alternating_prefix_is_outside_the_inner_list() {
    let source = "> - - x\n>     [r]: /url\n\nSee [r][].\n";

    // Both read `(2, 16, 23)`.
    assert_eq!(end(&node_at(source, "list", 5)), (1, 8, 7));
    assert_eq!(end(&node_at(source, "list_item", 3)), (1, 8, 7));
}

/// AND THE EMPTIED CONTAINER, which the ruling reached separately. The quote's
/// only content was the definition, so once the definition is not its child the
/// quote has NO placed child at all - and "ends at its last placed child" is
/// silent there. It spans the markup that opened it, `> `, and stops; the item
/// around it then ends where the quote does. Both read `(1, 12, 11)`, the end
/// of the line the definition was written on (markup-carve/carve-rs#1233).
#[test]
fn a_container_the_definition_emptied_spans_its_own_markup() {
    let source = "- > [^f]: t\n\nSee [^f].\n";

    assert_eq!(end(&node_at(source, "block_quote", 3)), (1, 5, 4));
    assert_eq!(end(&node_at(source, "list_item", 1)), (1, 5, 4));
}

// ---- a duplicate must not move the definition that was accepted ------------

/// A repeated label keeps the FIRST definition, and the later duplicate must
/// not move where that first one ends.
///
/// A definition followed by a blank line has its end pushed to the following
/// line, and that was applied by label rather than by definition - so the
/// duplicate on line 5 wrote line 6 onto the span of the definition on line 2.
///
/// THE FOOTNOTE HALF IS THE ONE THAT MATTERS and it did not move. The list half
/// read `(2, 10, 13)` because the list was widened onto the definition's line;
/// it now ends at its own item, which means the list no longer reports the bug
/// at all and the footnote's own span is what has to catch it.
#[test]
fn a_duplicate_definition_does_not_move_the_one_that_was_accepted() {
    let source = "- a\n  [^f]: t\ntail\n\n[^f]: dup\n\nx[^f]\n";

    assert_eq!(end(&node_at(source, "footnote", 3)), (2, 10, 13));
    assert_eq!(end(&node_at(source, "list", 1)), (1, 4, 3));
}

// ---- what the placeholder is there for --------------------------------------

/// The placeholder exists so the removed definition does not leave a BLANK line
/// that loosens the item (section 17 L1, carve#801). Moving it to the
/// definition's own column must not cost that: the lists stay tight and the
/// definition still renders nothing.
#[test]
fn moving_the_placeholder_keeps_the_list_tight() {
    let tight = html("- > - - x\n  >     [^f]: note\n\nSee [^f].\n");

    assert!(
        tight.contains("<li>x</li>"),
        "the innermost item was loosened, so the placeholder now reads as a second block:\n{tight}",
    );
    assert!(
        !tight.contains("[^f]"),
        "the definition line reached the output as text:\n{tight}",
    );
}

/// The same for the case with no structural prefix at all, which is the one
/// carve#801 was filed about - it is the branch the fix leaves alone, and a
/// regression there would be invisible in the spans. The continuation line
/// folds into the item's own paragraph, so what a loosened item would look
/// like here is a `<p>` wrapper, not a second block.
#[test]
fn a_definition_at_an_item_content_column_still_keeps_its_item_tight() {
    let tight = html("- a\n  [^f]: x\n  more\n\nSee [^f].\n");
    let list = &tight[..tight.find("</ul>").expect("no list in the output")];

    assert!(
        !list.contains("<p>"),
        "the item was loosened by the definition it hosted:\n{tight}",
    );
    assert!(
        list.contains("more"),
        "the continuation line no longer folds into the item:\n{tight}",
    );
}
