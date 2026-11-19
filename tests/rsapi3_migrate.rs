//! `migrateTree` (carve-rs#4335): the migrate clause cases and the JSON
//! boundary call.

use carve::api::{self, MigrateOptions, json::call};
use serde_json::{Value, json};

fn corpus() -> Value {
    let path = format!(
        "{}/spec-conformance/clause-cases.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut de = serde_json::Deserializer::from_str(&text);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).unwrap()
}

#[test]
fn c2_a_178_migrate_clause_cases() {
    let mut seen = 0;
    for c in corpus()["cases"].as_array().unwrap() {
        if c["kind"] != "migrate" {
            continue;
        }
        seen += 1;
        let source = c["options"]["source"].as_str().map(str::to_string);
        let migrated = api::migrate_tree_with(&c["tree"], &MigrateOptions { source })
            .unwrap_or_else(|e| panic!("{}: {e}", c["id"]));
        let mut got: Value = serde_json::from_str(&api::encode_tree(&migrated.tree)).unwrap();
        let mut want = c["expected"]["tree"].clone();
        // C2-A-162: equality skips srcByteLength, and pos where the case states none.
        let positions = want.to_string().contains("\"pos\"");
        for tree in [&mut got, &mut want] {
            geometry(tree, positions);
        }
        assert_eq!(got, want, "{}", c["id"]);
        if let Some(want) = c["expected"]["diagnostics"].as_array() {
            let codes: Vec<&str> = migrated
                .diagnostics
                .iter()
                .map(|d| d.code.as_str())
                .collect();
            let wanted: Vec<&str> = want.iter().filter_map(|d| d["code"].as_str()).collect();
            assert_eq!(codes, wanted, "{}", c["id"]);
        }
    }
    assert_eq!(seen, 5);
}

#[test]
fn migrate_tree_on_the_json_boundary() {
    let old = json!({"type": "document", "astVersion": "1.2", "children": [
        {"type": "list", "ordered": false, "tight": true, "items": [
            {"type": "list_item", "checked": true, "children": [
                {"type": "paragraph", "children": [{"type": "text", "value": "a"}]}]}]}]});
    let out: Value = serde_json::from_str(&call(
        &json!({"call": "migrateTree", "tree": old}).to_string(),
    ))
    .unwrap();
    assert_eq!(
        out["ok"]["tree"]["children"][0]["children"][0]["task"],
        json!({"mark": "x"})
    );
    assert_eq!(out["ok"]["diagnostics"], json!([]));
    assert!(out["ok"].get("envelope").is_none());
    let envelope = json!({"astVersion": "1.2", "document": old});
    let out: Value = serde_json::from_str(&call(
        &json!({"call": "migrateTree", "tree": envelope}).to_string(),
    ))
    .unwrap();
    assert_eq!(out["ok"]["envelope"]["astVersion"], "2.0");
    assert_eq!(out["ok"]["envelope"]["document"], out["ok"]["tree"]);
}

#[test]
fn migrate_tree_losses_and_refusals() {
    let old = json!({"type": "document", "astVersion": "1.2", "children": [
        {"type": "paragraph", "children": [{"type": "math", "value": "x", "label": "1"}]}]});
    let migrated = api::migrate_tree_with(&old, &MigrateOptions::default()).unwrap();
    assert_eq!(migrated.diagnostics.len(), 1);
    assert_eq!(migrated.diagnostics[0].code, "migration-loss");
    assert_eq!(
        migrated.diagnostics[0].path.as_deref(),
        Some("/children/0/children/0/label")
    );
    for bad in [
        json!({}),
        json!({"type": "document", "astVersion": "9.9", "children": []}),
    ] {
        let err = api::migrate_tree(&bad).unwrap_err();
        assert_eq!(err.spec_name(), "CarveTreeError", "{err}");
    }
}

fn geometry(value: &mut Value, positions: bool) {
    match value {
        Value::Object(map) => {
            map.remove("srcByteLength");
            if !positions {
                map.remove("pos");
            }
            map.values_mut().for_each(|v| geometry(v, positions));
        }
        Value::Array(items) => items.iter_mut().for_each(|v| geometry(v, positions)),
        _ => {}
    }
}

/// C2-A-178: the short caption of a figure is migrated like its caption.
#[test]
fn a_short_caption_is_migrated() {
    let old = json!({"type": "document", "astVersion": "1.2", "children": [{
        "type": "figure",
        "caption": [{"type": "text", "value": "C"}],
        "shortCaption": [{"type": "escaped_text", "value": "*"}],
        "children": [{"type": "code_block", "content": "x\n"}],
    }]});
    let migrated = api::migrate_tree(&old).unwrap();
    let tree: serde_json::Value = serde_json::from_str(&api::encode_tree(&migrated)).unwrap();
    assert_eq!(
        tree["children"][0]["caption"]["short"],
        json!([{"type": "text", "value": "*"}]),
        "{tree}"
    );
}
