//! C2-W-157: the path of a data-rule finding, by the kind of record the rule reports on.

use carve::lint::{LintOptions, lint};
use serde_json::Value;

fn path_of(input: &str, code: &str) -> Option<String> {
    lint(input, &LintOptions::default())
        .into_iter()
        .find(|finding| finding.code == code)
        .and_then(|finding| finding.path)
}

#[test]
fn the_model_states_a_path_for_each_record_kind() {
    let model: Value = serde_json::from_str(include_str!("../spec-data/lint-model.json")).unwrap();
    let mut kinds: Vec<&str> = model["paths"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    kinds.sort_unstable();
    assert_eq!(kinds, ["attribute", "block", "inline", "line", "text_run"]);
}

#[test]
fn a_block_record_names_its_node() {
    assert_eq!(
        path_of("- a\n\n b\n", "list-item-body-below-content-column").as_deref(),
        Some("/children/0/children/0/children/1")
    );
}

#[test]
fn a_text_run_names_the_node_that_holds_it() {
    assert_eq!(
        path_of("a **b** c\n", "markdown-strong-double-star").as_deref(),
        Some("/children/0")
    );
    assert_eq!(
        path_of("a **b\nc** d\n", "markdown-strong-double-star").as_deref(),
        Some("/children/0")
    );
}

#[test]
fn an_attribute_record_names_its_owner() {
    assert_eq!(
        path_of("{kbd}\n# H\n", "semantic-attribute-outside-span").as_deref(),
        Some("/children/0")
    );
}

#[test]
fn a_line_names_its_leaf_block() {
    // A verbatim leaf.
    assert_eq!(
        path_of("---\n>a\n---\n", "blockquote-marker-without-space").as_deref(),
        Some("/children/0")
    );
    // The row of a pipe table, here a row without `pos` that a continuation joined.
    assert_eq!(
        path_of("| a | %% x\n+ b |\n", "comment-splits-table-row").as_deref(),
        Some("/children/0/children/0/children/0")
    );
    // The paragraph inside a caption.
    assert_eq!(
        path_of("| a |\n^ cap\n  # h\n", "block-opener-without-blank-line").as_deref(),
        Some("/children/0/caption/children/0")
    );
}

#[test]
fn a_line_of_no_leaf_names_the_innermost_block_that_holds_it() {
    assert_eq!(
        path_of(":::\na\n::::\n", "colon-fence-length-mismatch").as_deref(),
        Some("/children/0")
    );
    assert_eq!(
        path_of("- a\n  :::\n  b\n  ::::\n", "colon-fence-length-mismatch").as_deref(),
        Some("/children/0/children/0/children/1")
    );
}
