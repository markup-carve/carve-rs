//! A container's extent reaches the DEFINITION it hosted.
//!
//! A footnote or link reference definition is lifted out of the body before the
//! block parser runs, and an invisible placeholder is left on the line it
//! opened so the container still sees a non-blank line there. Two things about
//! that placeholder left a container ending a line early, where carve-js and
//! carve-php agree with each other (carve-rs#1106):
//!
//! * behind an ALTERNATING container prefix - a quote marker carrying a list
//!   that is reached by plain indentation - the placeholder was put at the
//!   prefix's column instead of the definition's own, so every list deeper than
//!   the prefix closed on the line before;
//! * a MULTI-LINE definition leaves one placeholder for a body of several
//!   lines, so the container's cursor-derived span ended on the opening line.
//!
//! PART 12 section 4 is markup-inclusive (markup-carve/carve#913): a
//! container's extent has to cover the source it consumed.

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
/// marker in front of it, and its body runs one line further.
#[test]
fn a_list_reaches_the_end_of_the_body_of_the_definition_it_hosted() {
    let source = "- a\n  [^f]: t\n    more\ntail\n\nx[^f]\n";

    assert_eq!(end(&node_at(source, "list", 1)), (3, 9, 22));
}

/// Corpus 359. A single blank line inside the note body keeps it open, so the
/// list still reaches the body's last line.
#[test]
fn a_blank_line_inside_the_hosted_body_does_not_stop_the_list_early() {
    let source = "- a\n  [^f]: t\n\n    more\ntail\n\nx[^f]\n";

    assert_eq!(end(&node_at(source, "list", 1)), (4, 9, 23));
}

/// INTENDED SURVIVOR. Only the block that ENDS ON the definition's line is
/// widened, and the item ends on line 1 - its content is `a` and nothing else.
/// carve-js reports the same split, so widening the item too would trade one
/// divergence for another.
#[test]
fn the_item_that_ended_before_the_definition_is_left_where_it_was() {
    let source = "- a\n  [^f]: t\n    more\ntail\n\nx[^f]\n";

    assert_eq!(end(&node_at(source, "list_item", 1)), (1, 4, 3));
}

/// INTENDED SURVIVOR. A single-line definition has nothing past its own line,
/// so nothing widens: the list still ends on the line the definition opened.
#[test]
fn a_single_line_definition_does_not_widen_the_container_that_hosted_it() {
    let source = "- a\n  [^f]: t\ntail\n\nx[^f]\n";

    assert_eq!(end(&node_at(source, "list", 1)), (2, 10, 13));
}

// ---- an alternating container prefix ----------------------------------------

/// Corpus 360-...-2. `>` is the only structural prefix on the second line; the
/// two lists under it are reached by indentation, and the definition stands at
/// the innermost content column.
#[test]
fn a_footnote_definition_behind_an_alternating_prefix_is_inside_the_inner_list() {
    let source = "- > - - x\n  >     [^f]: note\n\nSee [^f].\n";

    assert_eq!(end(&node_at(source, "list", 7)), (2, 19, 28));
    assert_eq!(end(&node_at(source, "list_item", 5)), (2, 19, 28));
}

/// Corpus 360-...-4. The same shape for a LINK reference definition, which is
/// lifted out by the other prepass - both carried the same placeholder bug.
#[test]
fn a_link_definition_behind_an_alternating_prefix_is_inside_the_inner_list() {
    let source = "> - - x\n>     [r]: /url\n\nSee [r][].\n";

    assert_eq!(end(&node_at(source, "list", 5)), (2, 16, 23));
    assert_eq!(end(&node_at(source, "list_item", 3)), (2, 16, 23));
}

/// A SINGLE-LINE definition can leave a container short too. The placeholder
/// is not always as wide as the line it replaced, so the block that consumed
/// that line ended inside it: the item and the quote here stopped at column 8
/// of an 11-column line. Found by a mutation that removed the "multi-line
/// definitions only" filter and turned out to fix this rather than break it.
#[test]
fn a_single_line_definition_still_reaches_the_end_of_the_line_it_was_written_on() {
    let source = "- > [^f]: t\n\nSee [^f].\n";

    assert_eq!(end(&node_at(source, "list_item", 1)), (1, 12, 11));
    assert_eq!(end(&node_at(source, "block_quote", 3)), (1, 12, 11));
}

// ---- a duplicate must not move the definition that was accepted ------------

/// A repeated label keeps the FIRST definition, and the later duplicate must
/// not move where that first one ends.
///
/// A definition followed by a blank line has its end pushed to the following
/// line, and that was applied by label rather than by definition - so the
/// duplicate on line 5 wrote line 6 onto the span of the definition on line 2.
/// Nothing read that span until the widening above, which then reported the
/// list as running to line 6 and swallowing `tail` and the duplicate with it.
#[test]
fn a_duplicate_definition_does_not_move_the_one_that_was_accepted() {
    let source = "- a\n  [^f]: t\ntail\n\n[^f]: dup\n\nx[^f]\n";

    assert_eq!(end(&node_at(source, "footnote", 3)), (2, 10, 13));
    assert_eq!(end(&node_at(source, "list", 1)), (2, 10, 13));
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
