use carve::{parse, to_carve, to_html, to_json};
use serde_json::Value;

fn without_pos(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            fields.remove("pos");
            fields.remove("srcByteLength");
            for child in fields.values_mut() {
                without_pos(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                without_pos(item);
            }
        }
        _ => {}
    }
}

#[test]
fn span_marker_ast_and_carve_match_js() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/span_marker_astref.json")).unwrap();
    for case in cases {
        let source = case["s"].as_str().unwrap();
        let mut actual: Value = serde_json::from_str(&to_json(&parse(source))).unwrap();
        without_pos(&mut actual);
        assert_eq!(actual, case["ast"], "AST for {source:?}");
        assert_eq!(
            to_carve(source),
            case["fmt"].as_str().unwrap(),
            "Carve for {source:?}"
        );
    }
}

#[test]
fn span_marker_html_matches_spec_cases() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/span_marker_cases.json")).unwrap();
    for case in cases {
        let source = case["s"].as_str().unwrap();
        assert_eq!(
            to_html(source).trim(),
            case["html"].as_str().unwrap(),
            "HTML for {source:?}"
        );
    }
}

#[test]
fn column_offsets_survive_wire_and_collisions_are_unspellable() {
    let doc = parse("| a | < |\n+ x | y |\n");
    let mut wire: Value = serde_json::from_str(&to_json(&doc)).unwrap();
    let back = carve::from_json(&serde_json::to_string(&wire).unwrap()).unwrap();
    assert_eq!(back.children, doc.children);

    wire["children"][0]["rows"][0]["cells"][0]["children"][3]
        .as_object_mut()
        .unwrap()
        .remove("col");
    let colliding = carve::from_json(&serde_json::to_string(&wire).unwrap()).unwrap();
    assert!(carve::render_carve(&colliding).is_err());
}
