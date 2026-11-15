//! Shared fixtures for the integration tests.
#![allow(dead_code)]

use serde_json::Value;

/// One specification example with the oracle's tree (tests/data/examples.json).
pub struct Example {
    pub id: String,
    pub input: String,
    pub html: String,
    pub opts: Value,
    pub oracle_tree: Value,
}

pub fn examples() -> Vec<Example> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/examples.json"
    ))
    .unwrap_or_default();
    // serde_json stops at nesting depth 128 and one example nests deeper, so
    // the top-level array is split by hand and each example read on its own.
    split_array(&text)
        .into_iter()
        .filter_map(|s| serde_json::from_str::<Value>(s).ok())
        .map(|e| Example {
            id: e["id"].as_str().unwrap_or("").to_string(),
            input: e["input"].as_str().unwrap_or("").to_string(),
            html: e["html"].as_str().unwrap_or("").to_string(),
            opts: e["opts"].clone(),
            oracle_tree: e["oracleTree"].clone(),
        })
        .collect()
}

/// The elements of a top-level JSON array, as source slices.
fn split_array(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut in_str, mut esc, mut start) = (0usize, false, false, 0usize);
    for (i, b) in text.bytes().enumerate() {
        if in_str {
            match b {
                _ if esc => esc = false,
                b'\\' => esc = true,
                b'"' => in_str = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'[' | b'{' => {
                depth += 1;
                if depth == 2 {
                    start = i;
                }
            }
            b']' | b'}' => {
                if depth == 2 {
                    out.push(&text[start..=i]);
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    out
}

/// JSON with object keys sorted, for byte comparison.
pub fn sorted(v: &Value) -> String {
    fn walk(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut keys: Vec<_> = m.keys().collect();
                keys.sort();
                let mut out = serde_json::Map::new();
                for k in keys {
                    out.insert(k.clone(), walk(&m[k]));
                }
                Value::Object(out)
            }
            Value::Array(a) => Value::Array(a.iter().map(walk).collect()),
            other => other.clone(),
        }
    }
    serde_json::to_string(&walk(v)).unwrap_or_default()
}
