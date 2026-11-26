use carve::api::json::call;
use serde_json::{Value, json};

fn invoke(request: &Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

fn paragraph(children: &Value) -> Value {
    json!({"type":"document","astVersion":"2.0","children":[{"type":"paragraph","children":children}]})
}

#[test]
fn smart_dash_runs_and_loss_fields() {
    let em = json!({"type":"smart_punctuation","kind":"em_dash","glyph":"—","source":"---"});
    let output = invoke(
        &json!({"call":"write","tree":paragraph(&json!([em,{"type":"text","value":"--x-"}]))}),
    );
    assert_eq!(output["ok"]["output"], "---\\-\\-x-\n", "{output}");
}
