fn set_code_value(node: &mut serde_json::Value, value: &str) {
    match node {
        serde_json::Value::Array(children) => {
            for child in children {
                set_code_value(child, value);
            }
        }
        serde_json::Value::Object(fields) => {
            if fields.get("type").and_then(serde_json::Value::as_str) == Some("code") {
                fields.insert("value".into(), value.into());
            }
            for (key, child) in fields {
                if key != "pos" {
                    set_code_value(child, value);
                }
            }
        }
        _ => {}
    }
}

#[test]
fn markdown_code_spans_keep_significant_whitespace() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/markdown-code-span-whitespace.json")).unwrap();
    for case in cases {
        let template = case["template"].as_str().unwrap();
        let value = case["value"].as_str().unwrap();
        let expected = case["markdown"].as_str().unwrap();
        let mut tree: serde_json::Value =
            serde_json::from_str(&carve::to_json(&carve::parse(template))).unwrap();
        set_code_value(&mut tree, value);
        let doc = carve::from_json(&tree.to_string()).unwrap();
        assert_eq!(
            carve::render_markdown(&doc).unwrap(),
            expected,
            "template={template:?}, value={value:?}"
        );
    }
}
