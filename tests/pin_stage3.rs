//! Cases of the carve-rs pin move to carve next2 268e883b (carve#6385, carve#6388, carve#6392, carve#6394).
use carve::api::json::call;
use carve::api::{Carve, Options, extensions};
use serde_json::{Value, json};

/// carve#6385, C2-X-035: a node a registered definition inserted, with no renderer in the selected channel,
/// is written through its fallback and reports the code.
#[test]
fn c2_x_035_registered_node_without_renderer_reports_fallback() {
    let mut host = Carve::new();
    host.register(
        extensions::table_of_contents(extensions::TableOfContentsOptions::default())
            .without_renderers(),
    )
    .unwrap();
    let out = host
        .render_with_diagnostics("= A\n\n== B\n", &Options::default())
        .unwrap();
    let found: Vec<_> = out
        .diagnostics
        .iter()
        .filter(|d| d.code == "extension-fallback-rendered")
        .collect();
    assert_eq!(found.len(), 1, "{:?}", out.diagnostics);
    assert!(
        found[0].message.contains("\"carve.toc\""),
        "{}",
        found[0].message
    );
}

fn reply(request: &Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

fn written(tree: &Value) -> String {
    let r = reply(&json!({"call": "write", "tree": tree}));
    assert_eq!(r["ok"]["diagnostics"], json!([]), "{r}");
    r["ok"]["output"].as_str().unwrap().to_string()
}

fn formatted(source: &str) -> Value {
    reply(&json!({"call": "format", "source": source}))
}

fn doc(children: Value) -> Value {
    let mut tree = json!({"type": "document", "astVersion": "2.0"});
    tree["children"] = children;
    tree
}

fn text(v: &str) -> Value {
    json!({"type": "text", "value": v})
}

/// carve#6388, C2-W-088, C2-W-090: the bytes do not depend on where the text is split.
#[test]
fn brace_ending_a_text_node_inside_emphasis_takes_no_backslash() {
    let split = doc(
        json!([{"type": "paragraph", "children": [{"type": "emphasis",
        "children": [text("{"), text("-"), text("--"), text("-")]}]}]),
    );
    let joined = doc(
        json!([{"type": "paragraph", "children": [{"type": "emphasis", "children": [text("{----")]}]}]),
    );
    assert_eq!(written(&split), "/{-\\--\\-/\n");
    assert_eq!(written(&joined), "/{-\\--\\-/\n");
    let en_dash = json!({"type": "smart_punctuation", "kind": "en_dash", "glyph": "\u{2013}", "source": "--"});
    let strong = doc(json!([{"type": "paragraph", "children": [{"type": "strong",
        "children": [text("{"), text(">-"), en_dash]}]}]));
    assert_eq!(written(&strong), "*{>\\---*\n");
}

/// carve#6392, C2-W-044, C2-B-229: a task box or quote mark between the dots keeps the bare dot.
#[test]
fn bare_dot_behind_a_task_box_or_quote_mark_stays_bare() {
    for (source, carve) in [
        (". [ ] . a\n", ". [ ] . a\n"),
        (". > . a\n", ". > . a\n"),
        (". [x] > . a\n", ". [x] > . a\n"),
        (". 1. a\n", ". 1. a\n"),
    ] {
        let r = formatted(source);
        assert_eq!(r["ok"]["source"].as_str(), Some(carve), "{source:?}: {r}");
    }
}

/// carve#6394, C2-W-154: after a list whose last block takes a lazy line the shape has no spelling; after a
/// list that ends in a closed block, also a nested one, the paragraph is written directly below it.
#[test]
fn typed_opener_paragraph_below_a_list() {
    let em = json!({"type": "smart_punctuation", "kind": "em_dash", "glyph": "\u{2014}", "source": "---"});
    let sb = json!({"type": "soft_break"});
    let para =
        json!({"type": "paragraph", "children": [em, text("yaml"), sb, text("k: v"), sb, em]});
    let item = |block: Value| {
        json!({"type": "list", "marker": {"kind": "bullet", "char": "-"},
        "spacing": "tight", "children": [{"type": "list_item", "children": [block]}]})
    };
    let code = json!({"type": "code_block", "value": "x\n"});
    assert_eq!(
        written(&doc(json!([item(item(code)), para]))),
        "- - ```\n    x\n    ```\n---yaml\nk: v\n---\n"
    );
    for source in [
        "- > = H\n---yaml\nk: v\n---\n",
        "- > ```\n  > x\n  > ```\n---yaml\nk: v\n---\n",
    ] {
        let r = formatted(source);
        assert_eq!(r["ok"]["source"].as_str(), Some(source), "{source:?}: {r}");
    }
    let quote = json!({"type": "block_quote", "children": [{"type": "paragraph", "children": [text("q")]}]});
    for last in [quote, json!({"type": "paragraph", "children": [text("a")]})] {
        let r = reply(&json!({"call": "write", "tree": doc(json!([item(last), para]))}));
        let diagnostics = &r["error"]["diagnostics"];
        assert_eq!(diagnostics.as_array().map(Vec::len), Some(1), "{r}");
        assert_eq!(diagnostics[0]["code"], "structure-unspellable", "{r}");
        assert_eq!(diagnostics[0]["path"], "/children/1", "{r}");
    }
}
