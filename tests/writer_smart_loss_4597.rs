use carve::api::json::call;
use serde_json::{Value, json};

fn invoke(request: Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

fn paragraph(children: Value) -> Value {
    json!({"type":"document","astVersion":"2.0","children":[{"type":"paragraph","children":children}]})
}

#[test]
fn smart_punctuation_loss_names_kind() {
    let em = json!({"type":"smart_punctuation","kind":"em_dash","glyph":"—","source":"---"});
    let en = json!({"type":"smart_punctuation","kind":"en_dash","glyph":"–","source":"--"});
    let output = invoke(
        json!({"call":"writeWithDiagnostics","tree":paragraph(json!([{"type":"text","value":"x"},en,em,{"type":"text","value":"y"}]))}),
    );
    assert_eq!(output["ok"]["output"], "x-----y\n", "{output}");
    let paths: Vec<_> = output["ok"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["path"].as_str())
        .collect();
    assert!(paths.contains(&"/children/0/children/1/kind"), "{output}");
    assert!(paths.contains(&"/children/0/children/2/kind"), "{output}");
}
