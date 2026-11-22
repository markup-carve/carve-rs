//! C2-W-117: a loss early in a block does not hide a later repair or field loss.
use carve::api::{self, Carve, WriteOptions};
use serde_json::{Value, json};

fn write(children: &[Value]) -> (String, Vec<String>) {
    let input = json!({"type":"document", "astVersion":"2.0", "extensions":[{"name":"carve.source","version":"1","required":false,"slots":[]}], "children":[
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
    let (output, losses) = write(&[
        json!({"type":"text", "value":" w"}),
        json!({"type":"text", "value":"a *b*"}),
    ]);
    assert_eq!(output, " wa \\*b*\n");
    assert_eq!(losses, ["field-unspellable /children/0/children/0/value"]);
}

#[test]
fn field_after_lost_indentation() {
    let (output, losses) = write(&[
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
    let (output, losses) = write(&[
        json!({"type":"text", "value":" w"}),
        json!({"type":"emphasis", "children":[{"type":"text", "value":"b "}]}),
    ]);
    assert_eq!(output, " w{/b /}\n");
    assert_eq!(losses, ["field-unspellable /children/0/children/0/value"]);
}

#[test]
fn standard_data_after_loss_is_not_a_field_loss() {
    let (output, losses) = write(&[
        json!({"type":"text", "value":" w"}),
        json!({"type":"text", "value":"a", "data":{"carve.source":{"format":"html", "locator":{"part":"input"}, "importer":{"name":"x", "version":"1"}}}}),
    ]);
    assert_eq!(output, " wa\n");
    assert_eq!(losses, ["field-unspellable /children/0/children/0/value"]);
}

#[test]
fn a_break_field_is_checked_before_resuming_on_the_next_line() {
    let (_, losses) = write(&[
        json!({"type":"text", "value":"a\r"}),
        json!({"type":"soft_break", "dir":"rtl"}),
        json!({"type":"text", "value":"x"}),
    ]);
    assert!(
        losses
            .iter()
            .any(|entry| entry == "field-unspellable /children/0/children/1/dir"),
        "{losses:?}"
    );
}
