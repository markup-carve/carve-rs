//! carve#6772: the C2-W-088 rounds read every reference from the tree, and the verbatim texts (C2-W-075) go back
//! after them, so the escapes beside a reference do not change between `fmt` passes (C2-W-003).
use carve::api::json::call;
use serde_json::{Value, json};

fn formatted(source: &str) -> String {
    let r: Value = serde_json::from_str(&call(
        &json!({"call": "format", "source": source}).to_string(),
    ))
    .unwrap();
    r["ok"]["source"].as_str().unwrap().to_string()
}

fn stable(source: &str, expected: &str) {
    let mut current = source.to_string();
    for pass in 1..=3 {
        current = formatted(&current);
        assert_eq!(current, expected, "pass {pass} of {source:?}");
    }
}

#[test]
fn escapes_beside_a_verbatim_reference_stay_on_the_next_pass() {
    stable(
        "^[/\\`[`[/[^]]][<]\n",
        "^\\[\\/\\`[\\`\\[\\/\\[^\\]\\]][<]\n",
    );
}

#[test]
fn a_reference_keeps_its_verbatim_text_beside_tree_rounds() {
    stable("> [\\`x\\[**`][x]\\[]\n", "> [\\`x\\[**`][x][]\n");
}
