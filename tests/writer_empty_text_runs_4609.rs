//! carve-rs#4609, C2-A-040, C2-A-161: an empty plain text beside a nonempty one is the same tree
//! without it, so the writer writes what it writes for the coalesced tree, with no loss.

use carve::api::{self, Carve, WriteOptions};
use serde_json::{Value, json};

fn write(children: &[Value]) -> (String, Vec<String>) {
    let tree = json!({ "type": "document", "astVersion": "2.0",
        "children": [{ "type": "paragraph", "children": children }] });
    let tree = api::decode_tree(&tree.to_string()).expect("a valid tree");
    let written = Carve::new()
        .write_with_diagnostics(&tree, &WriteOptions::default())
        .expect("a write");
    let losses = written
        .diagnostics
        .iter()
        .map(|e| format!("{} {}", e.code, e.path.clone().unwrap_or_default()))
        .collect();
    (written.output, losses)
}

fn text(v: &str) -> Value {
    json!({ "type": "text", "value": v })
}

#[test]
fn an_empty_text_between_a_code_span_and_a_text_is_nothing() {
    let code = json!({ "type": "code", "value": "c" });
    let with = write(&[code.clone(), text(""), text("`x`")]);
    assert_eq!(with, ("`c`\\`x`\n".to_string(), Vec::new()));
    assert_eq!(with, write(&[code, text("`x`")]));
}

#[test]
fn an_empty_first_text_of_a_strong_writes_as_the_coalesced_tree() {
    let strong = |kids: Vec<Value>| json!({ "type": "strong", "children": kids });
    let with = write(&[strong(vec![text(""), text("*"), text("a")])]);
    assert!(with.1.is_empty(), "{:?}", with.1);
    assert_eq!(with, write(&[strong(vec![text("*a")])]));
}

#[test]
fn a_run_of_empty_texts_only_keeps_its_loss() {
    let (_, losses) = write(&[text(""), text("")]);
    assert!(!losses.is_empty());
}
