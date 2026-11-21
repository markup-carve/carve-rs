//! C2-W-069, C2-W-117: a loss earlier in a block does not leave a later bare emphasis or strong that
//! cannot pair unbraced and unreported (carve-rs#4572).

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

fn pair(kind: &str, v: &str) -> Value {
    json!({ "type": kind, "children": [text(v)] })
}

fn soft_break() -> Value {
    json!({ "type": "soft_break" })
}

#[test]
fn carve_rs_4572_reproduction() {
    let (output, losses) = write(&[soft_break(), text("a"), pair("emphasis", "b ")]);
    assert_eq!(output, "\na{/b /}\n");
    assert_eq!(losses, ["structure-unspellable /children/0/children/0"]);
    let (output, _) = write(&[soft_break(), text("é"), pair("strong", " w")]);
    assert_eq!(output, "\né{* w*}\n");
}

#[test]
fn a_word_beside_the_marks_braces_the_pair() {
    let (output, _) = write(&[soft_break(), text("a"), pair("strong", "b")]);
    assert_eq!(output, "\na{*b*}\n");
    let (output, _) = write(&[soft_break(), text("a"), pair("emphasis", "b"), text("c")]);
    assert_eq!(output, "\na{/b/}c\n");
}

#[test]
fn a_loss_on_the_same_line_braces_the_pair() {
    let (output, losses) = write(&[text(" w"), pair("emphasis", "b ")]);
    assert_eq!(output, " w{/b /}\n");
    assert_eq!(losses, ["field-unspellable /children/0/children/0/value"]);
}

#[test]
fn a_block_without_a_loss_keeps_its_spelling() {
    let (output, losses) = write(&[text("z"), pair("strong", "  "), pair("strong", "y")]);
    assert_eq!(output, "z{*  *}{*y*}\n");
    assert_eq!(losses, Vec::<String>::new());
    let (output, _) = write(&[text("z "), pair("strong", "y")]);
    assert_eq!(output, "z *y*\n");
}
