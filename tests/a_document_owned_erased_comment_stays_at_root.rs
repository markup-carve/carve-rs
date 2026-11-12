//! An erased `%%` line the ownership walk gives to the document stays a
//! document block, even where a list's span reaches over the blanks around it
//! (carve#5012, CARVE-P11-079 (b), corpus 191).

use carve::{parse_with_options, to_carve, Options};
use serde_json::Value;

fn top_level_types(src: &str) -> Vec<String> {
    let doc = parse_with_options(src, &Options::default());
    let json: Value = serde_json::from_str(&carve::ast_json::to_json(&doc)).expect("ast json");
    json["children"]
        .as_array()
        .expect("children")
        .iter()
        .map(|block| block["type"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn a_comment_below_a_nested_item_after_its_last_line_is_a_document_block() {
    for src in [
        "- - a\n %% c\n\n b\n",
        "- - a\n%% c\n\nb\n",
        "- - a\n\n%% c\n\nb\n",
        "- - a\n\n %% c\n b\n",
    ] {
        assert_eq!(
            top_level_types(src),
            ["list", "comment", "paragraph"],
            "{src:?}"
        );
    }
}

#[test]
fn the_writer_puts_it_at_column_zero_and_settles() {
    let once = to_carve("- - a\n %% c\n\n b\n");
    assert_eq!(once, "- - a\n\n%% c\n\nb\n");
    assert_eq!(to_carve(&once), once);
}

/// Its column reaches no item, so the deeper line after it does not move it
/// into the item (carve#5178, carve#6003).
#[test]
fn a_comment_between_two_blocks_of_one_item_below_its_column_is_the_documents() {
    assert_eq!(top_level_types("- a\n\n%% c\n\n  b\n"), ["list", "comment"],);
}
