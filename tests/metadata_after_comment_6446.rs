//! carve#6446: a comment-only line between a blank line and a typed opener (C2-B-137, C2-B-232).
use carve::api::json::call;
use serde_json::{Value, json};

fn types(source: &str) -> Vec<String> {
    let r: Value = serde_json::from_str(&call(
        &json!({"call": "parse", "source": source}).to_string(),
    ))
    .unwrap();
    r["ok"]["tree"]["children"]
        .as_array()
        .unwrap_or_else(|| panic!("{source:?}: {r}"))
        .iter()
        .map(|c| c["type"].as_str().unwrap().to_string())
        .collect()
}

fn formatted(source: &str) -> String {
    let r: Value = serde_json::from_str(&call(
        &json!({"call": "format", "source": source}).to_string(),
    ))
    .unwrap();
    r["ok"]["source"].as_str().unwrap().to_string()
}

#[test]
fn typed_opener_behind_a_comment_after_a_blank_line_opens_a_metadata_block() {
    let source = "a\n\n%% c\n---yaml\nk: v\n---\n";
    assert_eq!(types(source), ["paragraph", "comment", "frontmatter"]);
    let r: Value = serde_json::from_str(&call(
        &json!({"call": "parse", "source": source}).to_string(),
    ))
    .unwrap();
    let meta = &r["ok"]["tree"]["children"][2];
    assert_eq!(meta["format"], "yaml");
    assert_eq!(meta["value"], "k: v\n");
    assert_eq!(formatted(source), "a\n\n%% c\n\n---yaml\nk: v\n---\n");
}

#[test]
fn comment_lines_below_a_container_keep_the_blank_line() {
    for (source, want) in [
        (
            "a\n\n%% c\n%% d\n---toml\nk: v\n---\n",
            &["paragraph", "comment", "comment", "frontmatter"][..],
        ),
        (
            "- c d\n\n  %% c\n---toml\nk: v\n---\n",
            &["list", "frontmatter"][..],
        ),
        (
            ":: t\n: c d\n\n  %% c\n---toml\nk: v\n---\n",
            &["definition_list", "frontmatter"][..],
        ),
        (
            "[^n]: q\n\n  %% c\n---json\nk: v\n---\n",
            &["footnote", "frontmatter"][..],
        ),
    ] {
        assert_eq!(types(source), want, "{source:?}");
    }
}

/// C2-B-232: with no paragraph open, a column-0 opener closes a description list as it closes a list.
#[test]
fn typed_opener_below_a_description_list_and_a_blank_line_opens_a_metadata_block() {
    assert_eq!(
        types(":: t\n: c d\n\n---toml\nk: v\n---\n"),
        ["definition_list", "frontmatter"]
    );
}

#[test]
fn without_the_blank_line_the_opener_stays_a_paragraph_line() {
    assert_eq!(types("a\n%% c\n---yaml\nk: v\n---\n"), ["paragraph"]);
}
