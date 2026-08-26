//! A property the schema pins with `const` admits ONE value, and that value is
//! checked AT DECODE (PART 12 S12(d)).
//!
//! This engine decodes field by field, so a wrong TYPE was already refused
//! (`bareMarker must be a boolean`) - but a wrong VALUE of the right type was
//! read for the value the decoder wanted and anything else discarded.
//! `optional_bool(obj, "bareMarker")?.unwrap_or(false)` turned `false` into
//! "not a bare marker", and `optional_string(obj, "mode")? == Some("integral")`
//! turned `"bogus"` into "not integral". Both decoded cleanly, the caller was
//! told nothing, and the value never reached the tree (carve-rs#1332).
//!
//! THAT IS A SILENT REPAIR, not a silent republish. carve-js put the invalid
//! value back on the wire (markup-carve/carve-js#1418); this engine normalized
//! it away instead. S12(d) names both halves as the same objection, and
//! carve-php's own error text spells it out: a reader that supplies a default
//! has silently repaired the payload, and one that reads a wrong type has
//! silently reinterpreted it.
//!
//! WHY `false` IS NOT A LESSER SPELLING OF ABSENT. The schema writes `const`
//! exactly where a field's PRESENCE is the fact. `definition_list.loose` is
//! `const: true` because absent means each description derives its own wrapper
//! from its block count, so there is no `false` to write and `loose: false`
//! states the OPPOSITE of what the field means. `strong.boldItalic`,
//! `list.bareMarker` and `citation_group.mode` are the same arrangement.
//!
//! ALL FOUR are covered, not just the one the defect was noticed on. They share
//! one generated table and one runtime check, so a fixture covering
//! `definition_list.loose` alone would pass while the other three stayed
//! unchecked.

use carve::{from_json, to_json};

const DOC: &str = r#"{"type":"document","srcByteLength":9,"children":[NODES]}"#;

fn decode(nodes: &str) -> Result<carve::Document, carve::AstJsonError> {
    from_json(&DOC.replace("NODES", nodes))
}

/// EVERY case is evaluated before anything fails.
///
/// A loop that asserts per iteration stops at the first field, leaving the
/// other three unmeasured - they did not pass, they never ran. Four fields
/// share one table and one check here, so that is exactly the coverage this
/// file exists to prove.
fn all_of(checks: impl IntoIterator<Item = Option<String>>) {
    let failures: Vec<String> = checks.into_iter().flatten().collect();
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

fn accepts(nodes: &str) -> carve::Document {
    decode(nodes).unwrap_or_else(|error| panic!("refused {nodes}: {error}"))
}

// The four `const`-pinned properties, each as a node the schema admits.
// `VALUE` is substituted, so the legal spelling and every illegal one run
// through the identical shape and nothing else can explain a difference.

const STRONG: &str = r#"{"type":"paragraph","children":[{"type":"strong","boldItalic":VALUE,"children":[{"type":"emphasis","children":[{"type":"text","value":"x"}]}]}]}"#;

const LIST: &str = r#"{"type":"list","ordered":true,"bareMarker":VALUE,"tight":true,"items":[{"type":"list_item","children":[{"type":"paragraph","children":[{"type":"text","value":"x"}]}]}]}"#;

const DEFINITION_LIST: &str = r#"{"type":"definition_list","loose":VALUE,"items":[{"type":"definition_term","children":[{"type":"text","value":"t"}]},{"type":"definition_description","children":[{"type":"paragraph","children":[{"type":"text","value":"d"}]}]}]}"#;

const CITATION_GROUP: &str = r#"{"type":"paragraph","children":[{"type":"citation_group","mode":VALUE,"raw":"[@a]","items":[{"type":"citation","key":"a","suppressAuthor":false,"pos":{"startLine":1,"endLine":1,"startColumn":1,"endColumn":3,"startOffset":0,"endOffset":2}}]}]}"#;

/// Every case: the shape, the property, its one legal value, and the node with
/// the property removed entirely.
const CASES: &[(&str, &str, &str, &str)] = &[
    ("strong", "boldItalic", "true", STRONG),
    ("list", "bareMarker", "true", LIST),
    ("definition_list", "loose", "true", DEFINITION_LIST),
    ("citation_group", "mode", "\"integral\"", CITATION_GROUP),
];

/// The wrong value of the RIGHT type - `false` for a `const: true`, another
/// string for a `const: "integral"`.
///
/// This is the case the old decoder could not see and a type check still
/// cannot: the type is correct and only the value is wrong.
#[test]
fn a_wrong_value_of_the_right_type_is_refused() {
    all_of(CASES.iter().map(|(ty, field, legal, shape)| {
        let wrong = if *legal == "true" {
            "false"
        } else {
            "\"bogus\""
        };
        match decode(&shape.replace("VALUE", wrong)) {
            Ok(_) => Some(format!("{ty}.{field}={wrong} was ACCEPTED")),
            Err(error) => {
                let message = error.to_string();
                let named = message.contains(ty)
                    && message.contains(field)
                    && message.contains("PART 12 §12(d)")
                    && message.contains(legal);
                (!named).then(|| {
                    format!("{ty}.{field}={wrong} refused without naming the violation: {message}")
                })
            }
        }
    }));
}

/// And the wrong TYPE, which was already refused but with a message that did
/// not say what the schema wanted.
#[test]
fn a_wrong_type_is_refused() {
    // A CONTROL: these were already refused before the const check existed, by
    // the decoder's own `must be a boolean` / `must be a string`. It holds on
    // both sides of the A/B, which is what makes it evidence that the new walk
    // did not take over a rule that already had an owner.
    all_of(CASES.iter().flat_map(|(ty, field, _legal, shape)| {
        ["null", "5", "[]", "{}"].into_iter().map(move |wrong| {
            decode(&shape.replace("VALUE", wrong))
                .is_ok()
                .then(|| format!("{ty}.{field}={wrong} was ACCEPTED"))
        })
    }));
}

/// The REPUBLISH half, asserted on its own.
///
/// A decode that let the value through would put it back on the wire, so the
/// payload would round-trip through this engine carrying a value the schema
/// calls invalid. Stated as "nothing leaves the engine at all", which also
/// rules out the weaker outcome this engine actually had: accepting the value
/// and silently normalizing it away.
#[test]
fn an_invalid_value_is_never_republished() {
    all_of(CASES.iter().map(|(ty, field, legal, shape)| {
        let wrong = if *legal == "true" {
            "false"
        } else {
            "\"bogus\""
        };
        decode(&shape.replace("VALUE", wrong)).ok().map(|document| {
            let republished = to_json(&document);
            let echoed = republished.contains(&format!("\"{field}\":{wrong}"));
            format!(
                "{ty}.{field}={wrong} DECODED (republished={echoed}); \
                 an accepted invalid value is either echoed back or silently repaired"
            )
        })
    }));
}

/// The near-miss an over-correction would also refuse.
///
/// A `const` makes the property OPTIONAL with one admitted value, not required.
/// Refusing an absent one would reject nearly every tree this engine's own
/// encoder writes, which S9(a) forbids.
#[test]
fn the_legal_value_and_the_absent_field_both_decode() {
    all_of(CASES.iter().flat_map(|(ty, field, legal, shape)| {
        // The property removed entirely, by deleting the `"field":VALUE,` pair
        // the shape spells - the same document minus one optional property.
        let without = shape.replace(&format!("\"{field}\":VALUE,"), "");
        let removed = if without.contains("VALUE") {
            Some(format!(
                "{ty}.{field}: the absent spelling kept the property"
            ))
        } else {
            None
        };
        [
            decode(&shape.replace("VALUE", legal))
                .err()
                .map(|e| format!("{ty}.{field}={legal} was REFUSED: {e}")),
            removed,
            decode(&without)
                .err()
                .map(|e| format!("{ty}.{field} absent was REFUSED: {e}")),
        ]
    }));
}

/// The legal value SURVIVES the round trip, so the check cannot be passing by
/// dropping the field on the way in.
#[test]
fn the_legal_value_still_rides_the_wire() {
    all_of(CASES.iter().map(|(ty, field, legal, shape)| {
        let republished = to_json(&accepts(&shape.replace("VALUE", legal)));
        (!republished.contains(&format!("\"{field}\":{legal}")))
            .then(|| format!("{ty}.{field}={legal} did not survive the round trip: {republished}"))
    }));
}

/// The generated table is what the runtime check consults, so the entries
/// reaching it are the thing under test.
///
/// `type` must NOT appear. Every node's `type` is a schema `const`, so a
/// generator that did not exclude it would claim all of them - and S12(c)
/// already rules on a node's type with its own error. Two producers of one rule
/// is the hazard, not the gap.
#[test]
fn the_generated_table_pins_four_properties_and_never_type() {
    let generated = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/wire_fields.rs"),
    )
    .expect("src/wire_fields.rs is readable");

    let table = generated
        .split_once("WIRE_CONST_FIELDS")
        .expect("the const table is generated")
        .1;

    all_of(CASES.iter().flat_map(|(ty, field, legal, _shape)| {
        let escaped = legal.replace('"', "\\\"");
        [
            (!table.contains(&format!("(\"{field}\", ")))
                .then(|| format!("{ty}.{field} is missing from the generated const table")),
            (!table.contains(&escaped))
                .then(|| format!("{ty}.{field} is in the table without its value {legal}")),
        ]
    }));

    assert!(
        !table.contains("(\"type\", "),
        "the const table claims `type`, which S12(c) already rules on"
    );
}
