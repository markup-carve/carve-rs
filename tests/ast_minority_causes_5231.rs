//! Source positions and scalar fields that diverged from both other engines
//! after the final AST pin (markup-carve/carve#5231).

use serde_json::{json, Value};

fn ast(source: &str) -> Value {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    serde_json::from_str(&carve::to_json(&carve::parse_with_options(
        source, &options,
    )))
    .unwrap()
}

#[test]
fn an_unclosed_verse_comment_owns_the_host_closer() {
    for (source, path, start) in [
        (
            "::: |\na\n%%%\nc\n:::\n",
            "/children/0/children/0/children/1",
            8,
        ),
        ("::: |\n  %%%\nx\n:::\n", "/children/0/children/0", 8),
    ] {
        let tree = ast(source);
        let comment = tree.pointer(path).unwrap();
        assert!(comment["content"].as_str().unwrap().ends_with(":::"));
        assert_eq!(comment["pos"]["startOffset"], start);
        assert_eq!(
            comment["pos"]["endOffset"],
            source.trim_end().chars().count()
        );
    }
}

#[test]
fn a_blank_before_a_descendants_plus_keeps_its_parent_list_tight() {
    let tree = ast("- a\n  - - x\n\n    +\n    y\n");
    assert_eq!(
        tree.pointer("/children/0/items/0/children/1/tight"),
        Some(&json!(true))
    );
    // An opaque first block takes the opposite ownership path.
    let control = ast("- - ```\n\n  +\n  x\n");
    assert_eq!(control.pointer("/children/0/tight"), Some(&json!(false)));
    assert_eq!(
        control.pointer("/children/0/items/0/children/0/tight"),
        Some(&json!(true))
    );
}

#[test]
fn an_indented_fence_closer_ends_after_its_authored_run() {
    let tree = ast("[^f]: +\n```\na\n\n  ```\n  c\nsee\n\nx[^f]\n");
    assert_eq!(
        tree.pointer("/children/1/children/0/pos/endOffset"),
        Some(&json!(20))
    );
}

#[test]
fn a_lazy_quoted_list_starts_at_its_parent_content_column() {
    let tree = ast("[^f]: > a\n      +\n      - x\n\n[^f]\n");
    let list = "/children/1/children/0/children/1";
    assert_eq!(
        tree.pointer(&format!("{list}/pos/startOffset")),
        Some(&json!(20))
    );
    assert_eq!(
        tree.pointer(&format!("{list}/items/0/pos/startOffset")),
        Some(&json!(20))
    );
}

#[test]
fn a_braced_comment_ends_at_its_closer_even_with_a_tail() {
    for (source, end) in [
        ("- a\n  - b\n{% c\n  d %}\n  - z\n", 21),
        ("- a0\n  - b0\n{% c0\n  d0 %}  - z0\n", 25),
    ] {
        let tree = ast(source);
        let comment = tree
            .pointer("/children/0/items/0/children/1/items/0/children/0/children/2")
            .unwrap();
        assert_eq!(comment["pos"]["endOffset"], end);
    }
}
