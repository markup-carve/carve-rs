//! C2-W-117, C2-W-151: `lang`, `dir` and `metadata` on a supplied text node are never dropped without a
//! report, whether the text holds only line-end spaces or follows another text.

use carve::api::{self, Carve, WriteOptions};
use serde_json::{Value, json};

fn losses(tree: &Value) -> Vec<String> {
    let tree = api::decode_tree(&tree.to_string()).expect("a valid tree");
    let written = Carve::new()
        .write_with_diagnostics(&tree, &WriteOptions::default())
        .expect("a write");
    written
        .diagnostics
        .iter()
        .map(|e| format!("{} {}", e.code, e.path.clone().unwrap_or_default()))
        .collect()
}

fn text(v: &str) -> Value {
    json!({ "type": "text", "value": v })
}

fn fields() -> [(&'static str, Value); 3] {
    [
        ("lang", json!("de")),
        ("dir", json!("rtl")),
        ("metadata", json!({ "title": [text("t")] })),
    ]
}

fn paragraph(children: Vec<Value>) -> Value {
    json!({ "type": "document", "astVersion": "2.0", "children": [{ "type": "paragraph", "children": children }] })
}

#[test]
fn a_field_on_a_line_end_space_text_is_reported() {
    for (name, value) in fields() {
        let mut spaces = text("  ");
        spaces[name] = value;
        let tree = paragraph(vec![text("a"), spaces]);
        assert_eq!(
            losses(&tree),
            ["structure-unspellable /children/0"],
            "{name}"
        );
    }
}

#[test]
fn a_field_on_a_text_after_a_text_is_reported() {
    for (name, value) in fields() {
        let mut b = text("b");
        b[name] = value;
        let tree = paragraph(vec![text("a"), b]);
        assert_eq!(
            losses(&tree),
            ["structure-unspellable /children/0"],
            "{name}"
        );
    }
}

fn hosts(children: Vec<Value>) -> Vec<Value> {
    let doc =
        |block: Value| json!({ "type": "document", "astVersion": "2.0", "children": [block] });
    vec![
        doc(json!({ "type": "paragraph", "children": children.clone() })),
        doc(json!({ "type": "heading", "level": 2, "children": children.clone() })),
        doc(
            json!({ "type": "line_block", "children": [{ "type": "line", "children": children.clone() }] }),
        ),
        doc(
            json!({ "type": "paragraph", "children": [text("x "), { "type": "emphasis", "children": children }] }),
        ),
    ]
}

#[test]
fn every_variant_of_a_field_on_text_reports_a_loss() {
    let values = ["  ", "\t", " \t", "b", "b ", "b\t", "b c "];
    let mut checked = 0;
    for (name, field) in fields() {
        for value in values {
            let mut t = text(value);
            t[name] = field.clone();
            let soft = json!({ "type": "soft_break" });
            let shapes = [
                vec![t.clone()],
                vec![text("a"), t.clone()],
                vec![text("a "), t.clone(), soft.clone(), text("z")],
                vec![text("a"), t.clone(), text("z")],
                vec![t.clone(), text("z")],
                vec![text("a"), t.clone(), text("y"), t.clone()],
            ];
            for shape in shapes {
                for tree in hosts(shape) {
                    let found = losses(&tree);
                    assert!(!found.is_empty(), "{name} {value:?}: {tree}");
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 3 * 7 * 6 * 4);
}
