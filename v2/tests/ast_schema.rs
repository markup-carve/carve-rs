//! C2-A-165: the generated node types read and write every oracle tree of the
//! specification examples without losing a field.
mod common;

use carve2::ast::owned::Node;

#[test]
fn c2_a_165_generated_types_round_trip_oracle_trees() {
    let mut failures = Vec::new();
    for ex in common::examples() {
        match serde_json::from_value::<Node>(ex.oracle_tree.clone()) {
            Ok(node) => {
                let back = serde_json::to_value(&node).unwrap_or_default();
                if common::sorted(&back) != common::sorted(&ex.oracle_tree) {
                    failures.push(format!("{}: differs", ex.id));
                }
                // The arena types write the same JSON as the owned ones.
                let arena = carve2::Arena::new();
                let in_arena = node.in_arena(&arena);
                let again = serde_json::to_value(&in_arena).unwrap_or_default();
                if again != back {
                    failures.push(format!("{}: arena tree differs", ex.id));
                }
            }
            Err(e) => failures.push(format!("{}: {e}", ex.id)),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
