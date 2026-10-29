use super::verbatim_continuation_guards::html_tree;

pub(super) fn payloads(root: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut records = Vec::new();
    let mut stack = vec![(String::new(), root)];
    while let Some((path, node)) = stack.pop() {
        if let Some(kind) = node.get("type").and_then(serde_json::Value::as_str) {
            if matches!(
                kind,
                "code" | "raw_inline" | "literal_inline" | "math" | "code_block" | "raw_block"
            ) {
                records.push(serde_json::json!({"path":path,"type":kind,"payload":node.get("content").or_else(||node.get("value")),"format":node.get("format"),"display":node.get("display")}));
            }
        }
        match node {
            serde_json::Value::Array(values) => {
                for (i, value) in values.iter().enumerate() {
                    stack.push((format!("{path}.{i}"), value));
                }
            }
            serde_json::Value::Object(values) => {
                for (key, value) in values {
                    stack.push((format!("{path}.{key}"), value));
                }
            }
            _ => {}
        }
    }
    records.sort_by(|a, b| a["path"].as_str().unwrap().cmp(b["path"].as_str().unwrap()));
    records
}

#[test]
#[ignore = "rule A ledger: carve-rs#3525"]
fn footnote_containers_keep_paragraph_breaks_and_opaque_payloads() {
    let controls: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "fixtures/footnote-container-paragraph-breaks.json"
    ))
    .unwrap();
    assert_eq!(controls.len(), 281);
    for control in controls {
        let source = control["source"].as_str().unwrap();
        let doc = carve::parse(source);
        let written = carve::render_carve(&doc).unwrap();
        let expected = html_tree(control["html"].as_str().unwrap());
        for html in [
            carve::to_html(source),
            carve::render_html(&doc).unwrap(),
            carve::to_html(&written),
        ] {
            assert_eq!(html_tree(&html), expected, "{source:?}");
        }
        let before: serde_json::Value = serde_json::from_str(&carve::to_json(&doc)).unwrap();
        let after: serde_json::Value =
            serde_json::from_str(&carve::to_json(&carve::parse(&written))).unwrap();
        assert_eq!(payloads(&before), payloads(&after), "{source:?}");
        assert_eq!(
            carve::render_carve(&carve::parse(&written)).unwrap(),
            written,
            "{source:?}"
        );
        let report =
            carve::to_carve_with_report(source, carve::CheckedRenderOptions::default()).unwrap();
        assert_eq!(report.value, written, "{source:?}");
        assert!(report.losses.is_empty());
        assert_eq!(report.total_losses, 0);
        assert!(!report.truncated);
    }
}
