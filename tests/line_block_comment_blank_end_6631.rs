//! carve#6631: an unclosed comment block in an unclosed line block does not take the trailing blank line (C2-B-113, C2-A-151).

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
fn the_comment_ends_at_its_last_line() {
    let src = "::: |\n%%%\n\n";
    assert_eq!(offsets(src, "/children/0"), (0, 9));
    assert_eq!(offsets(src, "/children/0/children/0"), (6, 9));
}

#[test]
fn the_parent_fence_keeps_the_blank_line_in_the_comment() {
    let src = "::: |\n%%%\n\n:::\n";
    assert_eq!(offsets(src, "/children/0/children/0"), (6, 10));
}
