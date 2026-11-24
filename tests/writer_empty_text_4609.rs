use carve::api::json::call;
use serde_json::{Value, json};

fn invoke(request: Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

fn paragraph(children: Value) -> Value {
    json!({"type":"document","astVersion":"2.0","children":[{"type":"paragraph","children":children}]})
}

#[test]
fn empty_text_in_a_nonempty_run_is_invisible() {
    let strong = paragraph(
        json!([{"type":"strong","children":[{"type":"text","value":""},{"type":"text","value":"*"},{"type":"text","value":"a"}]}]),
    );
    let no_empty = paragraph(
        json!([{"type":"strong","children":[{"type":"text","value":"*"},{"type":"text","value":"a"}]}]),
    );
    assert_eq!(
        invoke(json!({"call":"write","tree":no_empty}))["ok"]["output"],
        "{**a*}\n"
    );
    let output = invoke(json!({"call":"write","tree":strong}));
    assert_eq!(output["ok"]["output"], "{**a*}\n", "{output}");
    assert_eq!(output["ok"]["diagnostics"], json!([]), "{output}");
    let code = paragraph(
        json!([{"type":"code","value":"c"},{"type":"text","value":""},{"type":"text","value":"`x`"}]),
    );
    let output = invoke(json!({"call":"write","tree":code}));
    assert_eq!(output["ok"]["output"], "`c`\\`x`\n", "{output}");
    assert_eq!(output["ok"]["diagnostics"], json!([]), "{output}");
}
