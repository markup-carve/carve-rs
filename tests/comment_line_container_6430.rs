//! C2-W-155, C2-B-137: a top-level line comment below an open container (carve#6430, carve-rs#4608).
use carve::api::{self, Carve, WriteOptions};
use serde_json::{Value, json};

fn text(v: &str) -> Value {
    json!({"type": "text", "value": v})
}

fn paragraph(v: &str) -> Value {
    json!({"type": "paragraph", "children": [text(v)]})
}

fn list(ch: &str, v: &str) -> Value {
    json!({"type": "list", "marker": {"kind": "bullet", "char": ch}, "spacing": "tight",
        "children": [{"type": "list_item", "children": [paragraph(v)]}]})
}

fn comment(v: &str) -> Value {
    json!({"type": "comment", "form": "line", "value": v})
}

fn typed() -> Value {
    let dash =
        json!({"type": "smart_punctuation", "kind": "em_dash", "glyph": "—", "source": "---"});
    json!({"type": "paragraph", "children": [dash, text("yaml"), {"type": "soft_break"}, text("k: v"),
        {"type": "soft_break"}, dash]})
}

fn written(children: &[Value]) -> (String, Vec<String>) {
    let tree = json!({"type": "document", "astVersion": "2.0", "children": children});
    let tree = api::decode_tree(&tree.to_string()).unwrap();
    let w = Carve::new()
        .write_with_diagnostics(&tree, &WriteOptions::default())
        .unwrap();
    let paths = w
        .diagnostics
        .iter()
        .map(|e| format!("{} {}", e.code, e.path.clone().unwrap_or_default()))
        .collect();
    (w.output, paths)
}

#[test]
fn comment_line_below_an_open_container_is_reported() {
    let footnote = json!({"type": "footnote", "key": "n", "namespace": "footnote", "children": [paragraph("x")]});
    let reference = json!({"type": "paragraph", "children": [text("a"),
        {"type": "footnote_ref", "key": "n", "namespace": "footnote", "number": 1}]});
    let description = json!({"type": "definition_list", "children": [
        {"type": "definition_term", "children": [text("t")]},
        {"type": "definition_description", "children": [paragraph("d")]}]});
    for (children, source, path) in [
        (
            vec![list("-", "b"), comment("c"), paragraph("y")],
            "- b\n\n%% c\n\ny\n",
            "/children/1",
        ),
        (
            vec![description, comment("c"), paragraph("y")],
            ":: t\n: d\n\n%% c\n\ny\n",
            "/children/1",
        ),
        (
            vec![reference, footnote, comment("c"), paragraph("y")],
            "a[^n]\n\n[^n]: x\n\n%% c\n\ny\n",
            "/children/2",
        ),
    ] {
        let paths = vec![format!("structure-unspellable {path}")];
        assert_eq!(written(&children), (source.to_string(), paths));
    }
}

#[test]
fn every_comment_line_of_a_run_the_list_reads_in_is_reported() {
    let children = vec![
        list("-", "a"),
        comment("c"),
        comment("d"),
        list("*", "b"),
        typed(),
    ];
    assert_eq!(
        written(&children),
        (
            " - a\n\n%% c\n%% d\n\n * b\n\n ---yaml\nk: v\n---\n".to_string(),
            vec![
                "structure-unspellable /children/1".to_string(),
                "structure-unspellable /children/2".to_string(),
            ],
        )
    );
}
