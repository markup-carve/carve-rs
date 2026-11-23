//! Cases of the carve-rs pin move to carve next2 693f4f7f (carve#6397, carve#6391).
use carve::api::json::call;
use serde_json::{Value, json};

fn reply(request: &Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

fn formatted(source: &str) -> Value {
    reply(&json!({"call": "format", "source": source}))
}

fn text(v: &str) -> Value {
    json!({"type": "text", "value": v})
}

fn opener_paragraph() -> Value {
    let em = json!({"type": "smart_punctuation", "kind": "em_dash", "glyph": "\u{2014}", "source": "---"});
    let sb = json!({"type": "soft_break"});
    json!({"type": "paragraph", "children": [em, text("yaml"), sb, text("k: v"), sb, em]})
}

/// carve#6397, C2-W-154: a footnote or description list that ends in a closed block takes the paragraph on the
/// line directly below it, as a list does.
#[test]
fn typed_opener_paragraph_below_a_closed_footnote_or_description() {
    for source in [
        "a[^n]\n\n[^n]: x\n\n  ```\n  y\n  ```\n---yaml\nk: v\n---\n",
        "a[^n]\n\n[^n]: x\n\n  = H\n---yaml\nk: v\n---\n",
        "a[^n]\n\n[^n]: x\n\n  ::: note\n  y\n  :::\n---yaml\nk: v\n---\n",
        ":: t\n: ```\n  y\n  ```\n---yaml\nk: v\n---\n",
        ":: t\n: | a | b |\n---yaml\nk: v\n---\n",
        ":: t\n: x\n\n  - ```\n    y\n    ```\n---yaml\nk: v\n---\n",
    ] {
        let r = formatted(source);
        assert_eq!(r["ok"]["source"].as_str(), Some(source), "{source:?}: {r}");
    }
}

/// carve#6397, C2-W-154: where the footnote or description ends in a block that takes a lazy line, the shape
/// keeps the loss report.
#[test]
fn typed_opener_paragraph_below_a_lazy_footnote_or_description_is_unspellable() {
    let para = json!({"type": "paragraph", "children": [text("x")]});
    let quote = json!({"type": "block_quote", "children": [para]});
    let list = json!({"type": "list", "marker": {"kind": "bullet", "char": "-"}, "spacing": "tight",
        "children": [{"type": "list_item", "children": [para]}]});
    let mut containers = Vec::new();
    for last in [para, quote, list] {
        containers.push(
            json!({"type": "footnote", "key": "n", "namespace": "footnote", "children": [last]}),
        );
        containers.push(json!({"type": "definition_list", "children": [
            {"type": "definition_term", "children": [text("s")]},
            {"type": "definition_description", "children": [{"type": "paragraph", "children": [text("s")]}]},
            {"type": "definition_term", "children": [text("t")]},
            {"type": "definition_description", "children": [last]},
        ]}));
    }
    for container in containers {
        let tree = json!({"type": "document", "astVersion": "2.0",
            "children": [container, opener_paragraph()]});
        let r = reply(&json!({"call": "write", "tree": tree}));
        let diagnostics = &r["error"]["diagnostics"];
        assert_eq!(diagnostics.as_array().map(Vec::len), Some(1), "{r}");
        assert_eq!(diagnostics[0]["code"], "structure-unspellable", "{r}");
        assert_eq!(diagnostics[0]["path"], "/children/1", "{r}");
    }
}

/// carve#6391, C2-W-088: a single hyphen is no potential opener of the hyphen kind, so it stays bare in a block
/// whose hyphen run takes an escape.
#[test]
fn single_hyphen_stays_bare_beside_an_escaped_run() {
    for source in [
        "é- [a]: /u\n:  >=\n](u)# ^}:x *é=}---2) \\---$\nb\n",
        "a - b \\---\n",
    ] {
        let r = formatted(source);
        assert_eq!(r["ok"]["source"].as_str(), Some(source), "{source:?}: {r}");
    }
}
