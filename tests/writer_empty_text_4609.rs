use carve::api::json::call;
use serde_json::{Value, json};

fn invoke(request: &Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

fn paragraph(children: &Value) -> Value {
    json!({"type":"document","astVersion":"2.0","children":[{"type":"paragraph","children":children}]})
}

#[test]
fn empty_text_in_a_nonempty_run_is_invisible() {
    let strong = paragraph(
        &json!([{"type":"strong","children":[{"type":"text","value":""},{"type":"text","value":"*"},{"type":"text","value":"a"}]}]),
    );
    let no_empty = paragraph(
        &json!([{"type":"strong","children":[{"type":"text","value":"*"},{"type":"text","value":"a"}]}]),
    );
    assert_eq!(
        invoke(&json!({"call":"write","tree":no_empty}))["ok"]["output"],
        "{**a*}\n"
    );
    let output = invoke(&json!({"call":"write","tree":strong}));
    assert_eq!(output["ok"]["output"], "{**a*}\n", "{output}");
    assert_eq!(output["ok"]["diagnostics"], json!([]), "{output}");
    let code = paragraph(
        &json!([{"type":"code","value":"c"},{"type":"text","value":""},{"type":"text","value":"`x`"}]),
    );
    let output = invoke(&json!({"call":"write","tree":code}));
    assert_eq!(output["ok"]["output"], "`c`\\`x`\n", "{output}");
    assert_eq!(output["ok"]["diagnostics"], json!([]), "{output}");
}

#[test]
fn loss_paths_follow_the_supplied_tree_after_a_text_run() {
    let tree = paragraph(&json!([
        {"type":"text","value":"a"},
        {"type":"text","value":"b"},
        {"type":"text","value":"c","attrs":{"keyValues":[["x","a\nb"]]}}
    ]));
    let output = invoke(&json!({"call":"writeWithDiagnostics","tree":tree}));
    let paths: Vec<String> = output["ok"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["path"].as_str().map(str::to_string))
        .collect();
    assert!(
        paths.contains(&"/children/0/children/2/attrs/keyValues/0/1".to_string()),
        "{output}"
    );
}
