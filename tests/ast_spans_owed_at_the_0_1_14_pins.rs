//! AST spans and comment content owed at the 0.1.14 pins (carve-rs#4276,
//! carve-rs#4274, carve-rs#4263). HTML is unchanged by every case here.

use serde_json::Value;

fn ast(source: &str) -> Value {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    serde_json::from_str(&carve::to_json(&carve::parse_with_options(
        source, &options,
    )))
    .expect("the serializer emits JSON")
}

fn at<'a>(node: &'a Value, path: &[&str]) -> &'a Value {
    path.iter()
        .fold(node, |node, step| match step.parse::<usize>() {
            Ok(index) => &node[index],
            Err(_) => &node[*step],
        })
}

fn offsets(node: &Value) -> (u64, u64) {
    let pos = &node["pos"];
    (
        pos["startOffset"].as_u64().expect("placed"),
        pos["endOffset"].as_u64().expect("placed"),
    )
}

#[test]
fn an_item_lead_comment_run_drops_its_continuation_indentation() {
    for (source, content) in [
        ("- {% a\n   {lang=de}\n   z %} y\n", "a\n{lang=de}\nz"),
        ("- {% a\n   ```\n   z %} y\n", "a\n```\nz"),
        ("1. {% a\n      b\n      c %} y\n", "a\nb\nc"),
    ] {
        let doc = ast(source);
        let comment = at(
            &doc,
            &[
                "children", "0", "items", "0", "children", "0", "children", "0",
            ],
        );
        assert_eq!(comment["type"], "comment", "{source:?}");
        assert_eq!(comment["content"], content, "{source:?}");
    }
}

#[test]
fn a_comment_after_a_refused_item_plus_stays_in_the_item() {
    let doc = ast("- +\n%% c\n\ntext\n");
    let item = at(&doc, &["children", "0", "items", "0"]);
    assert_eq!(item["children"][0]["type"], "comment");
    assert_eq!(offsets(item), (0, 8));
    assert_eq!(offsets(&doc["children"][0]), (0, 8));
    assert_eq!(doc["children"][1]["type"], "paragraph");

    // Before a sibling the comment stays in source order, inside the first item.
    let doc = ast("- +\n%% c\n- b\n");
    let list = &doc["children"][0];
    assert_eq!(offsets(list), (0, 12));
    assert_eq!(offsets(&list["items"][0]), (0, 8));
    assert_eq!(list["items"][0]["children"][0]["type"], "comment");
    assert_eq!(doc["children"].as_array().unwrap().len(), 1);
}

#[test]
fn the_second_line_of_a_lazy_brace_run_is_placed() {
    let doc = ast("- a\n  - b\n{% c\n  d %}\n  - c\n");
    let para = at(
        &doc,
        &[
            "children", "0", "items", "0", "children", "1", "items", "0", "children", "0",
        ],
    );
    let children = para["children"].as_array().unwrap();
    assert_eq!(children[3]["type"], "soft_break");
    assert_eq!(offsets(&children[3]), (14, 17));
    assert_eq!(children[4]["value"], "d %}");
    assert_eq!(offsets(&children[4]), (17, 21));
}

#[test]
fn a_quoted_note_plus_places_its_lazy_follower() {
    let doc = ast("> [^a]: +\n> {#i\n .y}\n\nsee[^a]\n");
    let note = &doc["children"][2];
    assert_eq!(note["type"], "footnote");
    let para = &note["children"][0];
    assert_eq!(offsets(para), (12, 20));
    assert_eq!(offsets(&para["children"][3]), (17, 20));
}

#[test]
fn a_losing_note_plus_places_the_winning_body() {
    let doc = ast("[^n]: +\n[^n]: +\n  para\n\nsee[^n]\n");
    let note = &doc["children"][1];
    assert_eq!(note["type"], "footnote");
    assert_eq!(offsets(note), (0, 22));
    assert_eq!(offsets(&note["children"][0]), (18, 22));
    assert_eq!(offsets(&note["children"][0]["children"][0]), (18, 22));
}
