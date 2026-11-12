//! The schema map keeps an inline literal's and an inline comment's text as
//! child content; `attrs.content` is the legacy form (markup-carve/carve-rs#4294).

use carve::{from_prosemirror, parse, render_carve, to_prosemirror};
use serde_json::{json, Value};

const SOURCE: &str = "!`A B` and %% note\n";

#[test]
fn the_text_is_written_as_child_content() {
    let pm: Value = serde_json::from_str(&to_prosemirror(&parse(SOURCE)).json).unwrap();
    assert_eq!(
        pm["content"][0]["content"],
        json!([
            {"type": "carveLiteral", "content": [{"type": "text", "text": "A B"}]},
            {"type": "text", "text": " and"},
            {"type": "carveCommentInline", "attrs": {"block": false, "delimited": false},
             "content": [{"type": "text", "text": "note"}]}
        ])
    );
}

#[test]
fn both_forms_read_back() {
    let current = json!({"type": "doc", "content": [{"type": "paragraph", "content": [
        {"type": "carveLiteral", "content": [{"type": "text", "text": "A B"}]},
        {"type": "text", "text": " and"},
        {"type": "carveCommentInline", "attrs": {"block": false, "delimited": false},
         "content": [{"type": "text", "text": "note"}]}]}]});
    let legacy = json!({"type": "doc", "content": [{"type": "paragraph", "content": [
        {"type": "carveLiteral", "attrs": {"content": "A B"}},
        {"type": "text", "text": " and"},
        {"type": "carveCommentInline", "attrs": {"block": false, "content": "note", "delimited": false}}]}]});
    for stored in [current, legacy] {
        let back = from_prosemirror(&stored.to_string()).unwrap();
        assert_eq!(render_carve(&back).unwrap(), SOURCE, "{stored}");
    }
}

#[test]
fn an_empty_literal_writes_no_text_node() {
    let pm: Value = serde_json::from_str(&to_prosemirror(&parse("a !`` b\n")).json).unwrap();
    let text = pm.to_string();
    assert!(!text.contains(r#""text":"""#), "{text}");
}
