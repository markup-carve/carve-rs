use carve::api::{self, Carve, EncodeOptions, Options};
use serde_json::Value;

fn tree(source: &str) -> Value {
    let carve = Carve::new();
    let parsed = carve.parse(source, &Options::default()).unwrap();
    serde_json::from_str(&api::encode_tree_with(
        &parsed,
        EncodeOptions { positions: false },
    ))
    .unwrap()
}

#[test]
fn comment_only_line_ends_open_editorial_comment() {
    let doc = tree("{#a\n%% b#}\n");
    let blocks = doc["children"].as_array().unwrap();
    assert_eq!(blocks[0]["type"], "paragraph");
    assert_eq!(blocks[0]["children"][0]["value"], "{");
    assert_eq!(blocks[0]["children"][1]["type"], "tag");
    assert_eq!(blocks[0]["children"][1]["name"], "a");
    assert_eq!(blocks[0]["children"][2]["type"], "soft_break");
    assert_eq!(blocks[0]["children"][3]["type"], "comment");
    assert_eq!(blocks[0]["children"][3]["value"], "b#}");
    assert_eq!(blocks[0]["children"][3]["form"], "line");
}

#[test]
fn interior_percent_run_does_not_end_editorial_comment() {
    let doc = tree("x {#a\nb %% c#}\n");
    let children = doc["children"][0]["children"].as_array().unwrap();
    assert_eq!(children[1]["type"], "critic_comment");
    assert_eq!(children[1]["value"], "a\nb %% c");
}
