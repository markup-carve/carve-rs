use carve::api::{self, Carve, EncodeOptions, Options, WriteOptions};
use serde_json::json;

#[test]
fn superscript_with_escaped_carets_and_braces_round_trips() {
    let source = "{^\\^\\{\\^\\\\{\\^\\{x^}\n";
    let carve = Carve::new();
    let written = carve
        .format_with_diagnostics(source, &WriteOptions::default())
        .unwrap();
    assert!(written.diagnostics.is_empty(), "{:?}", written.diagnostics);
    let tree = |source: &str| {
        let encoded = api::encode_tree_with(
            &carve.parse(source, &Options::default()).unwrap(),
            EncodeOptions { positions: false },
        );
        let mut value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        value.as_object_mut().unwrap().remove("srcByteLength");
        value
    };
    assert_eq!(tree(&written.output), tree(source));
}

#[test]
fn glued_span_loss_names_the_missing_child() {
    let input = json!({"type":"document","astVersion":"2.0","children":[{"type":"line_block","children":[{"type":"line","children":[{"type":"emphasis","children":[{"type":"math","display":false,"value":"."},{"type":"code","value":"c"}]}]}]}]});
    let tree = api::decode_tree(&input.to_string()).unwrap();
    let written = Carve::new()
        .write_with_diagnostics(&tree, &WriteOptions::default())
        .unwrap();
    let losses: Vec<_> = written
        .diagnostics
        .iter()
        .map(|e| (e.code.as_str(), e.path.as_deref()))
        .collect();
    assert_eq!(
        losses,
        [
            (
                "field-unspellable",
                Some("/children/0/children/0/children/0/children/0/value")
            ),
            (
                "structure-unspellable",
                Some("/children/0/children/0/children/0/children/1")
            )
        ]
    );
}
