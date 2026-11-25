//! carve#6627: attribute lines that no block takes become a paragraph that the containers around them own (C2-A-151, C2-B-192).

use carve::api;
use carve::options::Options;
use serde_json::Value;

fn offsets(src: &str, pointer: &str) -> (u64, u64) {
    let tree = api::parse(src, &Options::default()).unwrap();
    let value: Value = serde_json::from_str(&api::encode_tree(&tree)).unwrap();
    let pos = &value.pointer(pointer).unwrap()["pos"];
    (
        pos["startOffset"].as_u64().unwrap(),
        pos["endOffset"].as_u64().unwrap(),
    )
}

#[test]
fn the_list_ends_behind_the_attribute_line() {
    let src = "- {d=\"\"}\n---l\n";
    assert_eq!(offsets(src, "/children/0"), (0, 8));
    assert_eq!(offsets(src, "/children/0/children/0"), (0, 8));
}

#[test]
fn a_task_item_ends_behind_the_attribute_line() {
    let src = "- [ ] {d=\"\"}\n---l\n";
    assert_eq!(offsets(src, "/children/0"), (0, 12));
}
