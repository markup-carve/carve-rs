//! carve#6706: a table group whose last child is a comment ends behind that comment (C2-A-144).

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
fn the_head_ends_behind_its_comment() {
    assert_eq!(
        offsets("|= Name |\n%% note\n| Ada |\n", "/children/0/children/0"),
        (0, 17)
    );
    assert_eq!(
        offsets("|= a | %% note\n| b |\n", "/children/0/children/0"),
        (0, 14)
    );
    assert_eq!(
        offsets("|= a |\n%% n1\n%% n2\n| b |\n", "/children/0/children/0"),
        (0, 18)
    );
}

#[test]
fn a_nested_head_ends_behind_its_comment() {
    assert_eq!(
        offsets(
            "> |= a |\n> %% note\n> | b |\n",
            "/children/0/children/0/children/0"
        ),
        (2, 18)
    );
    assert_eq!(
        offsets(
            "- |= a |\n  %% note\n  | b |\n",
            "/children/0/children/0/children/0/children/0"
        ),
        (2, 18)
    );
}

#[test]
fn a_body_before_a_foot_ends_behind_its_comment() {
    assert_eq!(
        offsets(
            "{footer-rows=1}\n| a |\n%% note\n| f |\n",
            "/children/0/children/0"
        ),
        (16, 29)
    );
}
