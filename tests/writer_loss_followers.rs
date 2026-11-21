//! C2-W-117: a loss early in a block does not hide a later repair or field loss.
use carve::api::{self, Carve, WriteOptions};
use serde_json::{Value, json};

fn write(children: Vec<Value>) -> (String, Vec<String>) {
    let input = json!({"type":"document", "astVersion":"2.0", "children":[
        {"type":"paragraph", "children":children}
    ]});
    let tree = api::decode_tree(&input.to_string()).unwrap();
    let result = Carve::new()
        .write_with_diagnostics(&tree, &WriteOptions::default())
        .unwrap();
    let losses = result
        .diagnostics
        .iter()
        .map(|e| format!("{} {}", e.code, e.path.as_deref().unwrap_or("")))
        .collect();
    (result.output, losses)
}

#[test]
fn escape_after_lost_indentation() {
    let (output, losses) = write(vec![
        json!({"type":"text", "value":" w"}),
        json!({"type":"text", "value":"a *b*"}),
    ]);
    assert_eq!(output, " wa \\*b*\n");
    assert_eq!(losses, ["field-unspellable /children/0/children/0/value"]);
}

#[test]
fn field_after_lost_indentation() {
    let (output, losses) = write(vec![
        json!({"type":"text", "value":" \t"}),
        json!({"type":"text", "value":" w", "dir":"rtl"}),
    ]);
    assert_eq!(output, " \t w\n");
    assert!(
        losses
            .iter()
            .any(|x| x == "field-unspellable /children/0/children/0/value")
    );
    assert!(
        losses
            .iter()
            .any(|x| x == "field-unspellable /children/0/children/1")
    );
}

#[test]
fn pair_after_lost_indentation() {
    let (output, losses) = write(vec![
        json!({"type":"text", "value":" w"}),
        json!({"type":"emphasis", "children":[{"type":"text", "value":"b "}]}),
    ]);
    assert_eq!(output, " w{/b /}\n");
    assert_eq!(losses, ["field-unspellable /children/0/children/0/value"]);
}
