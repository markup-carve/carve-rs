//! C2-A-012, C2-A-029, C2-T-044: a tree decoded from an envelope keeps the envelope's declarations.

use carve::api::{self, TreeReadOptions};
use serde_json::{Value, json};

fn envelope() -> Value {
    json!({"astVersion":"2.0","extensions":[
        {"name":"x.note","version":"1","required":false,"slots":[]}],
        "document":{"type":"document","astVersion":"2.0","children":[
            {"type":"paragraph","data":{"x.note":{"by":"a"}},
             "children":[{"type":"text","value":"Hello"}]}]}})
}

fn sidecar() -> Value {
    json!({"ranges":[{"id":"r1","start":{"path":"/children/0/children/0","offset":0},
        "end":{"path":"/children/0/children/0","offset":5},"data":{"x.note":{"by":"b"}}}]})
}

fn decoded() -> api::Tree {
    api::decode_tree(&envelope().to_string()).expect("a valid envelope decodes")
}

#[test]
fn decoded_tree_validates() {
    api::validate_tree(&decoded()).expect("the decoded tree is valid");
}

#[test]
fn decoded_tree_validates_with_range_data() {
    let ranges = sidecar();
    api::validate_tree_with(
        &decoded(),
        TreeReadOptions {
            annotations: Some(&ranges),
        },
    )
    .expect("an envelope-only declaration covers range data");
}

#[test]
fn encoded_envelope_decodes_again() {
    let text = api::encode_envelope(&decoded());
    api::decode_tree(&text).expect("encodeEnvelope of a decoded envelope reads back");
}

#[test]
fn patch_on_decoded_tree_applies() {
    let patch = [json!({"op":"test","path":"/astVersion","value":"2.0"})];
    api::apply_patch(&decoded(), &patch, None).expect("a test operation changes nothing");
}

#[test]
fn decoded_tree_carries_envelope_declarations() {
    let tree: Value = serde_json::from_str(&api::encode_tree(&decoded())).unwrap();
    assert_eq!(
        tree["extensions"],
        json!([{"name":"x.note","version":"1","required":false,"slots":[]}])
    );
}

#[test]
fn encoded_tree_decodes_again() {
    let text = api::encode_tree(&decoded());
    let again = api::decode_tree(&text).expect("encodeTree of a decoded envelope reads back");
    assert_eq!(api::encode_tree(&again), text);
}

#[test]
fn root_entries_first_then_envelope_order() {
    let mut env = envelope();
    env["extensions"] = json!([
        {"name":"x.b","version":"1","required":false,"slots":[]},
        {"name":"x.note","version":"1","required":false,"slots":[]},
        {"name":"x.a","version":"1","required":false,"slots":[]}]);
    env["document"]["extensions"] =
        json!([{"name":"x.note","version":"1","required":false,"slots":[]}]);
    let tree = api::decode_tree(&env.to_string()).expect("decodes");
    let tree: Value = serde_json::from_str(&api::encode_tree(&tree)).unwrap();
    let names: Vec<&str> = tree["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["x.note", "x.b", "x.a"]);
}

#[test]
fn envelope_only_declaration_beside_root_declarations() {
    let mut env = envelope();
    env["document"]["extensions"] =
        json!([{"name":"x.other","version":"1","required":false,"slots":[]}]);
    api::decode_tree(&env.to_string()).expect("the envelope declares x.note");
}

#[test]
fn root_that_declares_everything_keeps_its_wire_form() {
    let mut env = envelope();
    let declarations = env["extensions"].clone();
    env["document"]["extensions"] = declarations;
    let bare = api::decode_tree(&env["document"].to_string()).expect("decodes");
    let tree = api::decode_tree(&env.to_string()).expect("decodes");
    assert_eq!(api::encode_tree(&tree), api::encode_tree(&bare));
}
