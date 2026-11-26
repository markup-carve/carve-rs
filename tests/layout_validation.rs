//! A reader must reject malformed sidecars before a writer consumes them.

use carve::api;
use serde_json::{Value, json};

#[test]
fn accepts_minimal_records_and_invalid_utf8_bytes() {
    let minimal = json!({"version":2,"encoding":"utf-8","source":"a","lineEndings":"none","bom":false,
        "nodes":[{"path":"/children/0","startByte":0,"endByte":1}]});
    api::validate_layout(&minimal).unwrap();
    let bytes = json!({"version":2,"encoding":"base64","source":"Yf9i","lineEndings":"none","bom":false,
        "nodes":[{"path":"/children/0","startByte":0,"endByte":3}]});
    api::validate_layout(&bytes).unwrap();
}

#[test]
fn rejects_duplicate_paths_unpaired_fences_and_unknown_facts() {
    let base = json!({"version":2,"encoding":"utf-8","source":"a","lineEndings":"none","bom":false,
        "nodes":[{"path":"/children/0","startByte":0,"endByte":1}]});
    let mut cases: Vec<(Value, &str)> = Vec::new();
    let mut duplicate = base.clone();
    let first = duplicate["nodes"][0].clone();
    duplicate["nodes"].as_array_mut().unwrap().push(first);
    cases.push((duplicate, "/nodes/1/path"));
    let mut fence = base.clone();
    fence["nodes"][0]["openerRaw"] = "```".into();
    cases.push((fence, "/nodes/0"));
    let mut unknown = base;
    unknown["nodes"][0]["leadingPipe"] = true.into();
    cases.push((unknown, "/nodes/0/leadingPipe"));
    for (layout, path) in cases {
        let error = api::validate_layout(&layout).unwrap_err();
        assert_eq!(error.spec_name(), "CarveLayoutError");
        assert_eq!(error.path().as_deref(), Some(path));
    }
}
