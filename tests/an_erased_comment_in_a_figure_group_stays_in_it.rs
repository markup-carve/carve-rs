//! An erased `%%` line inside a `::: figure` group is a block of that group,
//! as it is of every other colon container (carve#5012, corpus 493-2).

use carve::{parse_with_options, to_carve, Options};
use serde_json::Value;

fn ast(src: &str) -> Value {
    let doc = parse_with_options(src, &Options::default());
    serde_json::from_str(&carve::ast_json::to_json(&doc)).expect("ast json")
}

fn types(blocks: &Value) -> Vec<String> {
    blocks
        .as_array()
        .expect("blocks")
        .iter()
        .map(|block| block["type"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn the_comment_is_a_child_of_the_group() {
    for (src, children) in [
        ("::: figure\n%% hidden\n:::\n", &["comment"][..]),
        (
            "::: figure\n![a](b.png)\n%% hidden\n:::\n",
            &["paragraph", "comment"][..],
        ),
        (
            "::: figure\n%% hidden\n![a](b.png)\n:::\n",
            &["comment", "paragraph"][..],
        ),
        ("> ::: figure\n> %% hidden\n> :::\n", &["comment"][..]),
    ] {
        let json = ast(src);
        let mut top = &json["children"];
        if top[0]["type"] == "block_quote" {
            top = &top[0]["children"];
        }
        assert_eq!(types(top), ["figure_group"], "{src:?}");
        assert_eq!(types(&top[0]["children"]), children, "{src:?}");
    }
}

#[test]
fn the_writer_keeps_it_inside_the_group() {
    for src in [
        "::: figure\n%% hidden\n:::\n",
        "::: figure\n![a](b.png)\n\n%% hidden\n:::\n",
    ] {
        assert_eq!(to_carve(src), src);
    }
}
