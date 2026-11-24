use carve::api::{Carve, Options, WriteOptions};

fn semantic_tree(tree: &carve::api::Tree) -> serde_json::Value {
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                for key in ["pos", "srcByteLength", "stages"] {
                    map.remove(key);
                }
                for child in map.values_mut() {
                    strip(child);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    strip(item);
                }
            }
            _ => {}
        }
    }
    let mut value = serde_json::from_str(&carve::api::encode_tree(tree)).unwrap();
    strip(&mut value);
    value
}

#[test]
fn nested_link_labels_keep_text_brackets_that_read_back() {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    for source in [
        "[*[[*[x](u)]]](u)\n".to_string(),
        "^*[*[[*^*[*[[*^*[*[[*^*[*x](u)^]]](u)^]]](u)^]]](u)\n".to_string(),
    ] {
        let tree = carve.parse(&source, &Options::default()).unwrap();
        let supplied = carve.write_with_diagnostics(&tree, &opts).unwrap();
        assert_eq!(supplied.output, source);
        assert!(supplied.diagnostics.is_empty());
        let written = carve.format_with_diagnostics(&source, &opts).unwrap();
        assert_eq!(written.output, source);
        assert!(
            written.diagnostics.is_empty(),
            "{source:?}: {:?}",
            written.diagnostics
        );
        let again = carve.parse(&written.output, &Options::default()).unwrap();
        assert_eq!(
            carve.write(&again, &opts).unwrap(),
            carve.write(&tree, &opts).unwrap()
        );
    }
}

#[test]
fn nested_link_bracket_family_reads_back_without_losses() {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    let mut checked = 0;
    for depth in 1..=16 {
        for lead in ["", "p ", "> ", "- "] {
            for leaf in [
                "[x](u)", "[xy](u)", "[x](v)", "[x]()", "*[x](u)*", "/[x](u)/", "[x](u) z",
                "z [x](u)",
            ] {
                for destination in ["u", "v"] {
                    let mut body = leaf.to_string();
                    for _ in 0..depth {
                        body = format!("[*[[*{body}]]](u)");
                    }
                    if destination == "v" {
                        body = body.replacen("](u)", "](v)", 1);
                    }
                    let source = format!("{lead}{body}\n");
                    let original = carve.parse(&source, &Options::default()).unwrap();
                    let written = carve.format_with_diagnostics(&source, &opts).unwrap();
                    assert!(
                        written.diagnostics.is_empty(),
                        "{source:?}: {:?}",
                        written.diagnostics
                    );
                    let read_back = carve.parse(&written.output, &Options::default()).unwrap();
                    assert_eq!(
                        carve::api::encode_tree(&read_back),
                        carve::api::encode_tree(&original),
                        "{source:?}"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 1024);
}

#[test]
fn a_supplied_nested_link_does_not_claim_an_unread_pair() {
    let tree = carve::api::decode_tree(r#"{"type":"document","astVersion":"2.0","children":[{"type":"paragraph","children":[{"type":"link","href":"v","children":[{"type":"text","value":"["},{"type":"link","href":"u","children":[{"type":"text","value":"]"}]},{"type":"text","value":"["}]}]}]}"#).unwrap();
    let carve = Carve::new();
    let written = carve
        .write_with_diagnostics(&tree, &WriteOptions::default())
        .unwrap();
    if written.diagnostics.is_empty() {
        let parsed = carve.parse(&written.output, &Options::default()).unwrap();
        assert_eq!(semantic_tree(&parsed), semantic_tree(&tree));
    }
}

#[test]
fn unrelated_sibling_repair_does_not_escape_link_brackets() {
    let carve = Carve::new();
    let parsed = carve
        .parse("[*[[*[x](u)]]](u)\n", &Options::default())
        .unwrap();
    let mut value: serde_json::Value =
        serde_json::from_str(&carve::api::encode_tree(&parsed)).unwrap();
    value["children"][0]["children"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"type": "text", "value": "*x*"}));
    let tree = carve::api::decode_tree(&value.to_string()).unwrap();
    let result = carve
        .write_with_diagnostics(&tree, &WriteOptions::default())
        .unwrap();
    assert_eq!(result.output, "[*[[*[x](u)]]](u)\\*x*\n");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}
