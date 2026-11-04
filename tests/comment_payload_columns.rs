use serde_json::Value;

fn payloads(source: &str) -> Vec<String> {
    fn walk(node: &Value, found: &mut Vec<String>) {
        match node {
            Value::Object(fields) => {
                if fields.get("type").and_then(Value::as_str) == Some("comment") {
                    found.push(fields["content"].as_str().unwrap().to_owned());
                }
                for value in fields.values() {
                    walk(value, found);
                }
            }
            Value::Array(values) => {
                for value in values {
                    walk(value, found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    let doc: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    walk(&doc, &mut found);
    found
}

#[test]
fn comment_payload_columns_survive_formatting() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/comment-payload-columns.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let expected: Vec<String> = serde_json::from_value(case["expected"].clone()).unwrap();
        assert_eq!(payloads(source), expected, "{}", case["name"]);
        let formatted = carve::to_carve(source);
        assert_eq!(payloads(&formatted), expected, "formatted {}", case["name"]);
        assert_eq!(
            carve::to_carve(&formatted),
            formatted,
            "fixed point {}",
            case["name"]
        );
    }
}
