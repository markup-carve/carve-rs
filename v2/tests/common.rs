//! Shared fixtures for the integration tests.
#![allow(dead_code)]

use carve2::options::{HeadingIds, Options, Profile};
use serde_json::Value;

/// One specification example with the oracle's tree (tests/data/examples.json).
pub struct Example {
    pub id: String,
    pub input: String,
    pub html: String,
    pub opts: Value,
    pub oracle_tree: Value,
    pub spec_tree: Option<Value>,
}

#[must_use]
pub fn examples() -> Vec<Example> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/examples.json"
    ))
    .unwrap_or_default();
    let v = parse_json(&text);
    v.as_array()
        .into_iter()
        .flatten()
        .map(|e| Example {
            id: e["id"].as_str().unwrap_or("").to_string(),
            input: e["input"].as_str().unwrap_or("").to_string(),
            html: e["html"].as_str().unwrap_or("").to_string(),
            opts: e["opts"].clone(),
            oracle_tree: e["oracleTree"].clone(),
            spec_tree: e.get("specTree").cloned(),
        })
        .collect()
}

/// JSON with object keys sorted, for byte comparison.
#[must_use]
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

/// The result fields of `ast-results.json` by node type.
#[must_use]
pub fn results() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/spec-data/ast-results.json"
    ))
    .unwrap_or_default();
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let mut out = Vec::new();
    if let Some(nodes) = v["nodes"].as_object() {
        for (ty, fields) in nodes {
            for f in fields.as_array().into_iter().flatten() {
                if let Some(name) = f["field"].as_str() {
                    out.push((ty.clone(), name.to_string()));
                }
            }
        }
    }
    out
}

fn strip_with(v: &mut Value, fields: &[(String, String)], authored_spans: bool) {
    match v {
        Value::Object(m) => {
            let ty = m
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            // Counts in a table without span marks are authored (C2-A-093).
            let authored_spans = if ty == "table" {
                !m.get("children")
                    .is_some_and(|c| c.to_string().contains("\"span\":"))
            } else {
                authored_spans
            };
            for (t, f) in fields {
                // Gap HTML-017: `numberPrefix` is listed as a result but nothing derives it.
                if f == "numberPrefix" || (authored_spans && (f == "colspan" || f == "rowspan")) {
                    continue;
                }
                // A direct link or image keeps its destination; only a reference use resolves.
                let direct = (ty == "link" || ty == "image") && !m.contains_key("key");
                if *t == ty && !direct {
                    m.remove(f);
                }
            }
            for c in m.values_mut() {
                strip_with(c, fields, authored_spans);
            }
        }
        Value::Array(a) => a
            .iter_mut()
            .for_each(|c| strip_with(c, fields, authored_spans)),
        _ => {}
    }
}

/// Removes every result field of `ast-results.json` from a tree.
pub fn strip_results(v: &mut Value) {
    strip_with(v, &results(), false);
}

/// The engine options an example names (`opts` of examples.json).
pub fn options_of(opts: &Value) -> Options {
    let mut o = Options::default();
    if let Some(h) = opts["headingIds"].as_str().and_then(HeadingIds::from_name) {
        o.heading_ids = h;
    }
    if let Some(p) = opts["profile"].as_str().and_then(Profile::from_name) {
        o.profile = p;
    }
    if let Some(r) = opts["roundtrip"].as_bool() {
        o.roundtrip = r;
    }
    if let Some(r) = opts["rawHtml"].as_bool() {
        o.raw_html = r;
    }
    if let Some(s) = opts["social"].as_object() {
        o.social = Some(carve2::options::Social {
            mention_url: s
                .get("mentionUrl")
                .and_then(Value::as_str)
                .map(str::to_owned),
            tag_url: s.get("tagUrl").and_then(Value::as_str).map(str::to_owned),
        });
    }
    o
}

/// JSON without `serde_json`'s nesting limit: trees of deep inputs pass 128 levels.
#[must_use]
pub fn parse_json(text: &str) -> Value {
    let mut de = serde_json::Deserializer::from_str(text);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).unwrap_or(Value::Null)
}
