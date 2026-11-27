//! C2-A-214, C2-A-029: a range may carry data with qualified, declared keys (carve#6885).

use carve::api::{self, TreeReadOptions};
use serde_json::{Value, json};

fn tree() -> Value {
    json!({"type":"document","astVersion":"2.0","extensions":[
        {"name":"x.note","version":"1","required":false,"slots":[]}],
        "children":[{"type":"paragraph","children":[{"type":"text","value":"hello"}]}]})
}

fn sidecar(data: &Value) -> Value {
    json!({"ranges":[{"id":"r1","start":{"path":"/children/0","offset":0},
        "end":{"path":"/children/0","offset":5},"data":data}]})
}

fn decode(tree: &Value, ranges: &Value) -> Result<(), String> {
    api::decode_tree_with(
        &tree.to_string(),
        TreeReadOptions {
            annotations: Some(ranges),
        },
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn validate(tree: &Value, ranges: &Value) -> Result<(), String> {
    let decoded = api::decode_tree(&tree.to_string()).map_err(|e| e.to_string())?;
    api::validate_tree_with(
        &decoded,
        TreeReadOptions {
            annotations: Some(ranges),
        },
    )
    .map_err(|e| e.to_string())
}

#[test]
fn accepts_declared_data_and_keeps_it_unchanged() {
    let ranges = sidecar(&json!({"x.note":{"by":"a"}}));
    let before = ranges.to_string();
    decode(&tree(), &ranges).unwrap();
    validate(&tree(), &ranges).unwrap();
    assert_eq!(ranges.to_string(), before);
}

#[test]
fn round_trips_the_sidecar_through_json() {
    let ranges = sidecar(&json!({"x.note":{"by":"a","n":[1,2.5,null,true]}}));
    let read: Value = serde_json::from_str(&ranges.to_string()).unwrap();
    validate(&tree(), &read).unwrap();
    assert_eq!(read, ranges);
}

#[test]
fn accepts_a_declaration_on_the_envelope() {
    let mut bare = tree();
    let extensions = bare.as_object_mut().unwrap().remove("extensions").unwrap();
    let envelope = json!({"astVersion":"2.0","extensions":extensions,"document":bare});
    decode(&envelope, &sidecar(&json!({"x.note":{"by":"a"}}))).unwrap();
    let mut empty = bare.clone();
    empty["extensions"] = json!([]);
    let both = json!({"astVersion":"2.0","extensions":envelope["extensions"],"document":empty});
    decode(&both, &sidecar(&json!({"x.note":{"by":"a"}}))).unwrap();
}

#[test]
fn refuses_an_undeclared_data_key() {
    let mut bare = tree();
    bare["extensions"] = json!([]);
    let ranges = sidecar(&json!({"x.note":{"by":"a"}}));
    for err in [
        decode(&bare, &ranges).unwrap_err(),
        validate(&bare, &ranges).unwrap_err(),
    ] {
        assert!(err.contains("/ranges/0/data"), "{err}");
        assert!(err.contains("undeclared"), "{err}");
    }
}

#[test]
fn refuses_an_unqualified_key_a_non_object_and_a_node_inside_data() {
    let err = validate(&tree(), &sidecar(&json!({"note":1}))).unwrap_err();
    assert!(err.contains("/ranges/0/data"), "{err}");
    assert!(err.contains("qualified"), "{err}");
    let err = validate(&tree(), &sidecar(&json!(1))).unwrap_err();
    assert!(err.contains("/ranges/0/data"), "{err}");
    let node = json!({"x.note":{"type":"text","value":"a"}});
    let err = validate(&tree(), &sidecar(&node)).unwrap_err();
    assert!(err.contains("/ranges/0/data/x.note"), "{err}");
}

#[test]
fn keeps_the_range_otherwise_closed() {
    let mut ranges = sidecar(&json!({"x.note":{}}));
    ranges["ranges"][0]["kind"] = json!("comment");
    let err = validate(&tree(), &ranges).unwrap_err();
    assert!(err.contains("/ranges/0"), "{err}");
    let mut root = sidecar(&json!({"x.note":{}}));
    root["data"] = json!({"x.note":{}});
    assert!(validate(&tree(), &root).is_err());
    let mut endpoint = sidecar(&json!({"x.note":{}}));
    endpoint["ranges"][0]["start"]["data"] = json!({"x.note":{}});
    assert!(validate(&tree(), &endpoint).is_err());
}
