//! carve-rs#4807: a cell wrap names the text child that holds the joining space, and its offset
//! counts bytes of that child's value (C2-L-020, C2-L-021).

use carve::api;
use carve::options::Options;
use serde_json::{Value, json};

const CELL: &str = "/children/0/children/0/children/0/children/0";
const KIDS: &str = "/children/0/children/0/children/0/children/0/children/0/children";

fn cell_entry(src: &str) -> Value {
    let parsed = api::parse_with_layout(src, &Options::default()).unwrap();
    parsed.layout["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["path"] == CELL)
        .cloned()
        .unwrap()
}

#[test]
fn a_wrap_behind_inline_markup_names_its_text_child() {
    assert_eq!(
        cell_entry("| *s* x |\n+ b `k` c |\n+ d |\n"),
        json!({"path":CELL,"startByte":1,"endByte":26,"wraps":[
            {"line":1,"path":format!("{KIDS}/1"),"offset":3},
            {"line":2,"path":format!("{KIDS}/3"),"offset":3},
        ]})
    );
}

#[test]
fn a_wrap_offset_counts_the_value_behind_an_escape() {
    assert_eq!(
        cell_entry("| a\\| b |\n+ c |\n")["wraps"],
        json!([{"line":1,"path":format!("{KIDS}/0"),"offset":5}])
    );
}
