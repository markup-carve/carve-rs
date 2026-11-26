//! C2-A-165: the generated node types read and write every oracle tree of the
//! specification examples without losing a field.
mod common;
#[path = "support/holds.rs"]
mod holds;

use carve::ast::owned::Node;
use holds::{Gate, Holds};

/// `(example id, cause)` of the oracle trees.
const ORACLE_HELD: Holds = &[];

/// `(tree id, cause)` of the supplied trees.
const SUPPLIED_HELD: Holds = &[];

#[test]
fn c2_a_165_generated_types_round_trip_oracle_trees() {
    let mut gate = Gate::new("oracle trees", ORACLE_HELD);
    for ex in common::examples() {
        let mut failures = Vec::new();
        match serde_json::from_value::<Node>(ex.oracle_tree.clone()) {
            Ok(node) => {
                let back = serde_json::to_value(&node).unwrap_or_default();
                if common::sorted(&back) != common::sorted(&ex.oracle_tree) {
                    failures.push(format!("{}: differs", ex.id));
                }
                // The arena types write the same JSON as the owned ones.
                let arena = carve::Arena::new();
                let in_arena = node.in_arena(&arena);
                let again = serde_json::to_value(&in_arena).unwrap_or_default();
                if again != back {
                    failures.push(format!("{}: arena tree differs", ex.id));
                }
            }
            Err(e) => failures.push(format!("{}: {e}", ex.id)),
        }
        gate.problems(&ex.id, &failures);
    }
    gate.finish();
}

/// The supplied trees of the tree gate (`spec-trees/nodes.json`) read into the
/// generated types and write back unchanged.
#[test]
fn c2_a_165_generated_types_round_trip_supplied_trees() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/spec-trees/nodes.json"
    ))
    .unwrap();
    let v = common::parse_json(&text);
    let mut gate = Gate::new("supplied trees", SUPPLIED_HELD);
    for t in v["trees"].as_array().into_iter().flatten() {
        let id = t["id"].as_str().unwrap_or("");
        let mut failures = Vec::new();
        match serde_json::from_value::<Node>(t["tree"].clone()) {
            Ok(node) => {
                let back = serde_json::to_value(&node).unwrap_or_default();
                if common::sorted(&back) != common::sorted(&t["tree"]) {
                    failures.push(format!("{id}: differs"));
                }
            }
            Err(e) => failures.push(format!("{id}: {e}")),
        }
        gate.problems(id, &failures);
    }
    gate.finish();
}
