//! carve#6417, C2-W-044 with C2-B-043, C2-B-229: a list written directly behind an attributed
//! bare-dot marker takes digits; a task box or quote mark between them keeps the bare dot.
use carve::api::json::call;
use serde_json::{Value, json};

fn reply(request: &Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

/// The source's tree written back with no sidecar facts.
fn rewritten(source: &str) -> String {
    let parsed = reply(&json!({"call": "parse", "source": source}));
    let tree = parsed["ok"]["tree"].clone();
    assert!(tree.is_object(), "{parsed}");
    let r = reply(&json!({"call": "write", "tree": tree}));
    assert_eq!(r["ok"]["diagnostics"], json!([]), "{source:?}: {r}");
    r["ok"]["output"].as_str().unwrap().to_string()
}

#[test]
fn inner_list_behind_an_attributed_bare_dot_takes_digits() {
    for (source, carve) in [
        (".{#x} 1. a\n", ".{#x} 1. a\n"),
        (".{#x} [ ] 1. a\n", ".{#x} [ ] . a\n"),
        (".{#x} > 1. a\n", ".{#x} > . a\n"),
        (". 1. a\n", ". 1. a\n"),
    ] {
        assert_eq!(rewritten(source), carve, "{source:?}");
    }
}
