//! A comment block inside a line block: the lazy comment below it (carve#6464) and
//! the strip of its lines (carve#6465).

use carve::api::{self, Carve, EncodeOptions, Options, WriteOptions};
use serde_json::{Value, json};

fn tree(source: &str) -> Value {
    let carve = Carve::new();
    let encoded = api::encode_tree_with(
        &carve.parse(source, &Options::default()).unwrap(),
        EncodeOptions { positions: false },
    );
    let mut value: Value = serde_json::from_str(&encoded).unwrap();
    value.as_object_mut().unwrap().remove("srcByteLength");
    value
}

/// The children of the line block in the first item.
fn line_block(source: &str) -> Value {
    tree(source)["children"][0]["children"][0]["children"][0]["children"].clone()
}

fn fmt(source: &str) -> String {
    Carve::new()
        .format(source, &WriteOptions::default())
        .unwrap()
}

#[test]
fn c2_b_123_lazy_comment_after_a_comment_block_ends_the_line_above() {
    let source = "- ::: |\n  a\n  %%%\n  c\n  %%%\n%% x\n";
    assert_eq!(
        line_block(source),
        json!([
            {"type": "line", "children": [
                {"type": "text", "value": "a"},
                {"type": "comment", "form": "line", "value": "x"},
            ]},
            {"type": "comment", "form": "block", "value": "c"},
        ])
    );
    let written = fmt(source);
    assert_eq!(written, "- ::: |\n  a\n%% x\n  %%%\n  c\n  %%%\n  :::\n");
    assert_eq!(tree(&written), tree(source));
    assert_eq!(fmt(&written), written);
}

#[test]
fn c2_b_123_lazy_comments_after_comment_blocks_in_other_containers() {
    for source in [
        "> ::: |\n> a\n> %%%\n> %%%\n%% x\n> b\n",
        "1. ::: |\n   a\n   %%%\n   c\n   %%%\n   %%%\n   %%%\n%% x\n",
        "a[^1]\n\n[^1]: ::: |\n  b\n  %%%\n  c\n  %%%\n%% x\n",
    ] {
        let doc = tree(source);
        let text = doc.to_string();
        assert!(
            text.contains(r#"{"form":"line","type":"comment","value":"x"}"#),
            "{source:?} lost the comment: {text}"
        );
        let written = fmt(source);
        assert_eq!(tree(&written), doc, "{source:?} -> {written:?}");
        assert_eq!(fmt(&written), written);
    }
}

#[test]
fn c2_b_007_a_straddling_tab_in_a_comment_block_line_is_kept() {
    let source = "- ::: |\n\t%%%\n\tc\td\n\t%%%\n";
    assert_eq!(
        line_block(source),
        json!([{"type": "comment", "form": "block", "value": "\tc\td"}])
    );
    let written = fmt(source);
    assert_eq!(written, "- ::: |\n  %%%\n  \tc\td\n  %%%\n  :::\n");
    assert_eq!(tree(&written), tree(source));
}

#[test]
fn c2_b_130_comment_block_lines_are_stripped_by_the_container_only() {
    for (source, value) in [
        ("- ::: |\n    %%%\n    c\n      d\n    %%%\n", "  c\n    d"),
        ("1. ::: |\n\t%%%\n\t\tc\n\t%%%\n", "\t\tc"),
        ("- ::: |\n  %%%\n\tc\n  %%%\n", "\tc"),
        ("- - ::: |\n\t%%%\n\tc\n\t%%%\n", "c"),
    ] {
        let doc = tree(source).to_string();
        let expected = json!({"form": "block", "type": "comment", "value": value}).to_string();
        assert!(doc.contains(&expected), "{source:?}: {doc}");
    }
}
