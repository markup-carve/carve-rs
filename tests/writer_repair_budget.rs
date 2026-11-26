//! C2-W-133: one writer repair budget for the whole document, drawn on by every group.

use carve::api::{Carve, WriteOptions};
use serde_json::Value;

/// The quote of CC-1493, whose round now escapes all reported constructs.
fn heavy_quote() -> String {
    let path = format!(
        "{}/spec-conformance/clause-cases.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut de = serde_json::Deserializer::from_str(&text);
    de.disable_recursion_limit();
    let cases: Value = serde::Deserialize::deserialize(&mut de).unwrap();
    let case = cases["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "CC-1493")
        .unwrap();
    let input = case["input"].as_str().unwrap();
    input.split("\n\n").next().unwrap().to_string()
}

fn refused_paths(source: &str) -> Vec<String> {
    let written = Carve::new()
        .format_with_diagnostics(source, &WriteOptions::default())
        .unwrap();
    written
        .diagnostics
        .iter()
        .filter(|d| d.code == "writer-refused")
        .filter_map(|d| d.path.clone())
        .collect()
}

#[test]
fn one_heavy_group_fits_the_budget() {
    assert!(refused_paths(&format!("{}\n", heavy_quote())).is_empty());
}

#[test]
fn later_groups_complete_with_batched_repairs() {
    let quote = heavy_quote();
    let source = format!("{quote}\n\nplain\n\n{quote}\n\n{quote}\n");
    assert!(refused_paths(&source).is_empty());
}
