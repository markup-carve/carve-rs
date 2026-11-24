use carve::api::json::call;
use serde_json::{Value, json};

fn invoke(request: Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

#[test]
fn bracket_and_note_escape_rounds() {
    for (source, expected) in [
        ("^[[^\\^[b]\\]\n", "^\\[[^\\^[b]]\n"),
        ("^[\\]]} z\n", "^[\\]]} z\n"),
        ("a/b ^[\\]]} z\n", "a/b ^[\\]]} z\n"),
        ("- ^[\\]\\]*\\]a*]a\\[\n", "- ^[\\]\\]*\\]a*]a[\n"),
    ] {
        let output = invoke(json!({"call":"format","source":source}));
        assert_eq!(output["ok"]["source"], expected, "{source:?}: {output}");
    }
}
