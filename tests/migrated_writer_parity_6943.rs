//! Canonical source for seven 0.1 trees migrated to identical 2.0 trees (carve#6943).

use carve::api::{self, Carve, WriteOptions};
use serde_json::Value;

#[test]
fn migrated_trees_choose_the_shared_canonical_spelling() {
    let rows: Vec<Value> =
        serde_json::from_str(include_str!("data/migrated-writer-6943.json")).unwrap();
    let mut differences = Vec::new();
    for row in rows {
        let id = row["id"].as_str().unwrap();
        let tree = api::decode_tree(&row["tree"].to_string()).unwrap();
        let output = Carve::new()
            .write(&tree, &WriteOptions::default())
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        if output != row["source"].as_str().unwrap() {
            differences.push(format!(
                "{id}: expected {:?}, got {:?}",
                row["source"], output
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
