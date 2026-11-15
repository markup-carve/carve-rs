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
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/examples.json")).unwrap_or_default();
    let mut de = serde_json::Deserializer::from_str(&text);
    de.disable_recursion_limit();
    let v: Value = serde::Deserialize::deserialize(&mut de).unwrap_or(Value::Null);
    v.as_array()
        .into_iter()
        .flatten()
        .map(|e| Example {
            id: e["id"].as_str().unwrap_or("").to_string(),
            input: e["input"].as_str().unwrap_or("").to_string(),
            html: e["html"].as_str().unwrap_or("").to_string(),
            opts: e["opts"].clone(),
            oracle_tree: e["oracleTree"].clone(),
        })
        .collect()
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
