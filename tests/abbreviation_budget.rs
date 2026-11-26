//! C2-I-098: one parse-wide derived-text budget for copied expansions.

use carve::api::{self, Options};
use serde_json::Value;

fn abbreviations(value: &Value, out: &mut Vec<Value>) {
    match value {
        Value::Object(fields) => {
            if fields.get("type").and_then(Value::as_str) == Some("abbreviation") {
                out.push(value.clone());
            }
            for child in fields.values() {
                abbreviations(child, out);
            }
        }
        Value::Array(children) => {
            for child in children {
                abbreviations(child, out);
            }
        }
        _ => {}
    }
}

#[test]
fn exhausted_use_does_not_prevent_a_later_short_use() {
    let source = format!(
        "*[A]: {}\n*[B]: yyyyyyyyyyy\n*[C]: zzzzzzzzzz\n\nA A A A A A A A A A B C",
        "x".repeat(99_999)
    );
    let parsed = api::parse_with_diagnostics(source.as_str(), &Options::default()).unwrap();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let json: Value = serde_json::from_str(&api::encode_tree(&parsed.tree)).unwrap();
    let mut uses = Vec::new();
    abbreviations(&json, &mut uses);
    assert_eq!(uses.len(), 12);
    assert!(
        uses[..10]
            .iter()
            .all(|v| v["expansion"].as_str().is_some_and(|s| s.len() == 99_999))
    );
    assert!(uses[10].get("expansion").is_none());
    assert_eq!(uses[11]["expansion"], "zzzzzzzzzz");
}

#[test]
fn tree_json_size_stays_linear_with_repeated_long_expansion() {
    for n in [1_000, 2_000, 4_000] {
        let source = format!("*[A]: {}\n\n{}", "x".repeat(2 * n), "A ".repeat(n));
        let tree = api::parse(source.as_str(), &Options::default()).unwrap();
        let json = api::encode_tree(&tree);
        assert!(
            json.len() < 1_000_000 + 100 * source.len(),
            "n={n}: source {} bytes, tree {} bytes",
            source.len(),
            json.len()
        );
    }
}
