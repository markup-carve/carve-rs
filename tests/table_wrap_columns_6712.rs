//! carve#6712: a wrap of a cell over several columns records the covered column of its fragment
//! (C2-L-020, C2-L-048), and the writer puts each fragment back under that column (C2-W-067).

use carve::api::{self, Carve, WriteOptions};
use carve::options::Options;
use serde_json::Value;

fn wraps(src: &str) -> Vec<Value> {
    let parsed = api::parse_with_layout(src, &Options::default()).unwrap();
    parsed.layout["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|n| n.get("wraps").and_then(Value::as_array).cloned())
        .unwrap()
}

fn places(src: &str) -> Vec<(u64, Option<u64>, u64)> {
    wraps(src)
        .iter()
        .map(|w| {
            (
                w["line"].as_u64().unwrap(),
                w["col"].as_u64(),
                w["offset"].as_u64().unwrap(),
            )
        })
        .collect()
}

fn format(src: &str) -> String {
    Carve::new().format(src, &WriteOptions::default()).unwrap()
}

#[test]
fn records_col_on_every_wrap_of_a_two_column_cell_sorted_by_line_then_col() {
    assert_eq!(
        places("| a | < |\n+ | b |\n+ e | |\n"),
        [(1, Some(1), 4), (2, Some(0), 2)]
    );
    assert_eq!(
        places("| a | < |\n+ b | c |\n"),
        [(1, Some(0), 2), (1, Some(1), 4)]
    );
}

#[test]
fn counts_the_column_from_the_cells_own_also_behind_a_cell_of_its_row() {
    assert_eq!(
        places("| x | a | < | < |\n+ | | y | z |\n+ | q | | |\n"),
        [(1, Some(1), 4), (1, Some(2), 6), (2, Some(0), 2)]
    );
}

#[test]
fn records_no_col_for_a_cell_of_one_column() {
    assert!(
        wraps("| a | b |\n+ c | d |\n")
            .iter()
            .all(|w| w.get("col").is_none())
    );
}

#[test]
fn writes_each_fragment_back_under_its_column() {
    for src in [
        "| a | < |\n+ | b |\n+ e | |\n",
        "| a | < |\n+ b | c |\n",
        "| a | < |\n+ b | c |\n+ d | e |\n",
        "| a | < | < |\n+ x | y | z |\n+ | q | |\n",
        "> | x | a | < |\n> + | | b |\n> + c | e | |\n",
        "| x | a | < | < |\n+ | | y | z |\n+ | q | | |\n",
    ] {
        assert_eq!(format(src), src, "{src:?}");
    }
}

#[test]
fn writes_from_the_sidecar_as_from_the_source() {
    for src in [
        "| a | < |\n+ | b |\n+ e | |\n",
        "| a | < |\n+ b | c |\n+ d | e |\n",
    ] {
        let parsed = api::parse_with_layout(src, &Options::default()).unwrap();
        let opts = WriteOptions {
            layout: Some(parsed.layout),
            ..WriteOptions::default()
        };
        assert_eq!(Carve::new().write(&parsed.tree, &opts).unwrap(), src);
    }
}

#[test]
fn refuses_wraps_of_one_line_not_sorted_by_col() {
    let parsed = api::parse_with_layout("| a | < |\n+ b | c |\n", &Options::default()).unwrap();
    let mut layout = parsed.layout;
    let node = layout["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|n| n.get("wraps").is_some())
        .unwrap();
    node["wraps"].as_array_mut().unwrap().reverse();
    let error = api::validate_layout(&layout).unwrap_err().to_string();
    assert!(error.contains("not sorted by col"), "{error}");
}

#[test]
fn a_wrap_names_the_text_child_that_holds_its_joining_space() {
    let cell = "/children/0/children/0/children/0/children/1/children/0/children";
    assert_eq!(
        wraps("| y | `k` |\n+ | c |\n"),
        [serde_json::json!({"line": 1, "path": format!("{cell}/1"), "offset": 1})]
    );
    let first = wraps("| a | < |\n+ *s* | y |\n");
    assert_eq!(
        first[1]["path"],
        "/children/0/children/0/children/0/children/0/children/0/children/2"
    );
    assert_eq!(first[1]["offset"], 1);
}

#[test]
fn fragments_around_inline_nodes_and_escapes_write_back_under_their_columns() {
    for src in [
        "| c | < |\n+ | *s* |\n+ q r | `k` |\n",
        "| x | c | < |\n+ | `k` | `k` |\n+ | q r | a |\n",
        "| d | < | < |\n+ y | *s* | x |\n+ | `k` | |\n+ y | x | |\n",
    ] {
        assert_eq!(format(src), src, "{src:?}");
    }
    // The canonical writer drops the escape the star does not need.
    assert_eq!(
        format("| a\\*b | < |\n+ x\\|y | w *t* |\n"),
        "| a*b | < |\n+ x\\|y | w *t* |\n"
    );
}
