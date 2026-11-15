//! C2-A-133: resolving each oracle tree with its result fields stripped gives
//! the oracle tree back.
mod common;

use carve2::ast::Node;
use carve2::options::Options;
use serde_json::Value;

/// The result fields of `ast-results.json` by node type.
fn results() -> Vec<(String, String)> {
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

fn strip(v: &mut Value, fields: &[(String, String)]) {
    match v {
        Value::Object(m) => {
            let ty = m
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            for (t, f) in fields {
                // A direct link or image keeps its destination; only a reference use resolves.
                let direct = (ty == "link" || ty == "image") && !m.contains_key("key");
                if *t == ty && !direct {
                    m.remove(f);
                }
            }
            for c in m.values_mut() {
                strip(c, fields);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|c| strip(c, fields)),
        _ => {}
    }
}

pub fn options_of(opts: &Value) -> Options {
    let mut o = Options::default();
    if let Some(h) = opts["headingIds"]
        .as_str()
        .and_then(carve2::options::HeadingIds::from_name)
    {
        o.heading_ids = h;
    }
    o
}

#[test]
fn c2_a_133_resolve_restores_the_oracle_results() {
    let fields = results();
    let mut pass = 0;
    let mut failures = Vec::new();
    let all = common::examples();
    assert_eq!(all.len(), 374, "fixture not read");
    for ex in &all {
        let mut stripped = ex.oracle_tree.clone();
        strip(&mut stripped, &fields);
        let Ok(Node::Document(mut doc)) = serde_json::from_value::<Node>(stripped) else {
            failures.push(format!("{}: no document", ex.id));
            continue;
        };
        carve2::resolve::resolve(&mut doc, &options_of(&ex.opts));
        let back = serde_json::to_value(Node::Document(doc)).unwrap_or_default();
        if common::sorted(&back) == common::sorted(&ex.oracle_tree) {
            pass += 1;
        } else {
            failures.push(ex.id.clone());
        }
    }
    println!("resolve: {pass}/{} pass", all.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
