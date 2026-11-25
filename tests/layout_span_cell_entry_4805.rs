//! carve-rs#4805: a fragment that joins a cell through a span mark is a piece of that cell's entry.

use carve::api;
use carve::options::Options;
use serde_json::{Value, json};

const CELL: &str = "/children/0/children/0/children/0/children/0";
const TEXT: &str = "/children/0/children/0/children/0/children/0/children/0/children/0";

fn entries(src: &str) -> Vec<Value> {
    let parsed = api::parse_with_layout(src, &Options::default()).unwrap();
    parsed.layout["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n.get("wraps").is_some() || n.get("continuationLines").is_some())
        .cloned()
        .collect()
}

#[test]
fn a_colspan_cell_takes_the_fragment_under_its_mark() {
    assert_eq!(
        entries("| a | < |\n+ b | c |\n"),
        vec![
            json!({"path":"/children/0/children/0/children/0","startByte":0,"endByte":19,"continuationLines":1}),
            json!({"path":CELL,"startByte":1,"endByte":18,"wraps":[
                {"line":1,"path":TEXT,"offset":2},
                {"line":1,"path":TEXT,"offset":4},
            ]}),
        ]
    );
}

#[test]
fn the_wraps_of_a_colspan_cell_sort_by_line() {
    let nodes = entries("| a | < |\n+ b | c |\n+ d | e |\n");
    assert_eq!(
        nodes[1],
        json!({"path":CELL,"startByte":1,"endByte":28,"wraps":[
            {"line":1,"path":TEXT,"offset":2},
            {"line":1,"path":TEXT,"offset":6},
            {"line":2,"path":TEXT,"offset":4},
            {"line":2,"path":TEXT,"offset":8},
        ]})
    );
}

#[test]
fn a_rowspan_cell_names_the_row_of_its_fragment() {
    let nodes = entries("| a | x |\n| ^ | y |\n+ b | c |\n");
    assert_eq!(
        nodes[0],
        json!({"path":CELL,"startByte":1,"endByte":24,"wraps":[
            {"line":1,"path":TEXT,"offset":2,"row":1},
        ]})
    );
}
