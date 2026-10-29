use carve::{parse, to_html, to_html_with_options, to_json, Options};
use serde_json::Value;

fn strip_metadata(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("pos");
            object.remove("srcByteLength");
            object.remove("astVersion");
            for child in object.values_mut() {
                strip_metadata(child);
            }
        }
        Value::Array(children) => {
            for child in children {
                strip_metadata(child);
            }
        }
        _ => {}
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3525"]
fn ruled_quote_definition() {
    let rows: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/2990-quote-definition.json")).unwrap();
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let source = row["source"].as_str().unwrap();
        let expected_html = row["html"].as_str().unwrap();
        assert_eq!(to_html(source).trim(), expected_html, "{name}");
        assert_eq!(
            to_html_with_options(source, &Options::default().with_positions(true)).trim(),
            expected_html,
            "{name} with positions"
        );
        let mut actual_ast: Value = serde_json::from_str(&to_json(&parse(source))).unwrap();
        strip_metadata(&mut actual_ast);
        assert_eq!(actual_ast, row["ast"], "{name} AST");
    }
}
