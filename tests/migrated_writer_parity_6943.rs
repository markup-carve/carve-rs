//! Canonical source for seven 0.1 trees migrated to identical 2.0 trees (carve#6943).

use carve::api::{self, Carve, WriteOptions};
use serde_json::Value;
use serde_json::json;

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

#[test]
fn bracket_spelling_follows_a_paragraph_into_quotes_and_lists() {
    let rows: Vec<Value> =
        serde_json::from_str(include_str!("data/migrated-writer-6943.json")).unwrap();
    let paragraph = rows[1]["tree"]["children"][0].clone();
    let source = rows[1]["source"].as_str().unwrap();
    for (block, prefix) in [
        (
            json!({"type":"block_quote","children":[paragraph.clone()]}),
            "> ",
        ),
        (
            json!({"type":"list","marker":{"kind":"bullet","char":"-"},"spacing":"tight",
                "children":[{"type":"list_item","children":[paragraph]}]}),
            "- ",
        ),
    ] {
        let document = json!({"type":"document","astVersion":"2.0","children":[block]});
        let tree = api::decode_tree(&document.to_string()).unwrap();
        let output = Carve::new().write(&tree, &WriteOptions::default()).unwrap();
        assert_eq!(output, format!("{prefix}{source}"));
    }
}
