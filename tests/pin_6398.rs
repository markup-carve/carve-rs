//! Cases of the carve-rs pin move to carve next2 4afac79 (carve#6398, carve#6409, carve#6418): C2-W-155.
use carve::api::json::call;
use serde_json::{Value, json};

fn reply(request: &Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

fn formatted(source: &str) -> Value {
    reply(&json!({"call": "format", "source": source}))
}

/// C2-W-155: a lazy-ending list or description list is written one space in, and so is every list or
/// description list of the run above it, up to a block that is no open container, a block comment or an attribute
/// line.
#[test]
fn lazy_ending_container_and_run_above_are_indented() {
    for source in [
        " - b\n\n ---yaml\nk: v\n---\n",
        " a) x\n\n ---toml\na = 1\n",
        " - a\n\n - b\n\n ---json\n",
        " :: t\n : - x\n\n ---json\n",
        "p\n\n - b\n\n ---json\n",
        "> q\n\n :: t\n : d\n\n ---json\n",
        " - x\n\n :: t\n : d\n\n * b\n\n ---yaml\nk: v\n---\n",
        "a[^n]\n\n[^n]: x\n\n{#i}\n * b\n * c\n\n ---json\n",
        "- ```\n  x\n  ```\n\n{start=3}\n . b\n\n ---json\n",
        ":: t\n: d\n\n%%%\nsecret\n%%%\n\n - b\n\n ---yaml\nk: v\n---\n",
    ] {
        let r = formatted(source);
        assert_eq!(r["ok"]["source"].as_str(), Some(source), "{source:?}: {r}");
    }
}

/// C2-W-155, C2-W-117: a run that reaches a footnote has no spelling and reports the paragraph. A comment line
/// that the container above takes is reported instead, and the run is written as without it (carve#6430).
#[test]
fn run_into_footnote_or_across_comment_is_unspellable() {
    let comment = blocks("p\n\n%% c\n").pop().unwrap_or_default();
    for (mut children, path) in [
        (blocks("a[^n]\n\n[^n]: x\n\n:: t\n: d\n"), "/children/4"),
        (
            [blocks(":: t\n: d\n"), vec![comment]].concat(),
            "/children/1",
        ),
    ] {
        children.extend(blocks(" * b\n\n ---yaml\nk: v\n---\n"));
        let tree = json!({"type": "document", "astVersion": "2.0", "children": children});
        let r = reply(&json!({"call": "write", "tree": tree}));
        let diagnostics = &r["error"]["diagnostics"];
        assert_eq!(diagnostics.as_array().map(Vec::len), Some(1), "{r}");
        assert_eq!(diagnostics[0]["code"], "structure-unspellable", "{r}");
        assert_eq!(diagnostics[0]["path"], path, "{r}");
    }
}

fn blocks(source: &str) -> Vec<Value> {
    let r = reply(&json!({"call": "parse", "source": source}));
    r["ok"]["tree"]["children"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}
