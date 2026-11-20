use carve::api::{self, TreeReadOptions};
use serde_json::json;

#[test]
fn public_tree_calls_check_optional_annotation_ranges() {
    let tree = json!({"type":"document","astVersion":"2.0","extensions":[
        {"name":"carve.annotation","version":"1","required":false,"slots":[]}],
        "children":[{"type":"paragraph","children":[{"type":"text","value":"a"}],
            "data":{"carve.annotation":{"ids":["note"]}}}]});
    let ranges = json!({"ranges":[{"id":"note","start":{"path":"/children/0","offset":0},
        "end":{"path":"/children/0","offset":1}}]});
    let text = tree.to_string();
    let decoded = api::decode_tree_with(
        &text,
        TreeReadOptions {
            annotations: Some(&ranges),
        },
    )
    .unwrap();
    api::validate_tree_with(
        &decoded,
        TreeReadOptions {
            annotations: Some(&ranges),
        },
    )
    .unwrap();
    api::decode_tree(&text).unwrap();
    api::validate_tree(&decoded).unwrap();

    let duplicate = json!({"ranges":[ranges["ranges"][0], ranges["ranges"][0]]});
    let err = api::decode_tree_with(
        &text,
        TreeReadOptions {
            annotations: Some(&duplicate),
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("/ranges/1/id"), "{err}");
    let err = api::validate_tree_with(
        &decoded,
        TreeReadOptions {
            annotations: Some(&duplicate),
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("/ranges/1/id"), "{err}");
}

#[test]
fn sidecar_preserves_deep_tree_reader_depth() {
    let mut child = json!({"type":"paragraph","children":[{"type":"text","value":"a"}]});
    for _ in 0..130 {
        child = json!({"type":"block_quote","children":[child]});
    }
    let tree = json!({"type":"document","astVersion":"2.0","children":[child]});
    let text = tree.to_string();
    let empty = json!({"ranges":[]});
    api::decode_tree(&text).unwrap();
    let decoded = api::decode_tree_with(
        &text,
        TreeReadOptions {
            annotations: Some(&empty),
        },
    )
    .unwrap();
    api::validate_tree_with(
        &decoded,
        TreeReadOptions {
            annotations: Some(&empty),
        },
    )
    .unwrap();
}
