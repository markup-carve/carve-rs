use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use serde_json::{json, Value};

fn text(node: &Handle) -> String {
    match &node.data {
        NodeData::Text { contents } => contents.borrow().to_string(),
        _ => node.children.borrow().iter().map(text).collect(),
    }
}

fn code_records(node: &Handle, ancestors: &[String], records: &mut Vec<Value>) {
    let mut next = ancestors.to_vec();
    if let NodeData::Element { name, .. } = &node.data {
        let tag = name.local.to_string();
        if !matches!(tag.as_str(), "html" | "head" | "body" | "section") {
            next.push(tag.clone());
        }
        if tag == "code" {
            let elements: Vec<_> = node
                .children
                .borrow()
                .iter()
                .filter_map(|child| match &child.data {
                    NodeData::Element { name, .. } => Some(name.local.to_string()),
                    _ => None,
                })
                .collect();
            records.push(json!({ "value": text(node), "ancestors": next, "elements": elements }));
        }
    }
    for child in node.children.borrow().iter() {
        code_records(child, &next, records);
    }
}

#[test]
fn imported_code_payloads_keep_exact_text_and_containers() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/markdown-html-code-payloads.json")).unwrap();
    for case in cases {
        let source = carve::try_markdown_to_carve(case["markdown"].as_str().unwrap()).unwrap();
        let html = carve::to_html(&source);
        let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
        let mut records = Vec::new();
        code_records(&dom.document, &[], &mut records);
        assert_eq!(
            records,
            vec![json!({"value": case["value"], "ancestors": case["ancestors"], "elements": []})],
            "{}: {}",
            case["template"],
            case["value"]
        );
    }
}

#[test]
fn ast_import_retains_empty_and_multiline_code_nodes() {
    for (source, value) in [
        ("<code></code>", ""),
        ("<code>a<!---->&#10;<!---->b</code>", "a\nb"),
    ] {
        let document = carve::try_markdown_to_ast(source).unwrap();
        let carve::BlockNode::Paragraph(paragraph) = &document.children[0] else {
            panic!("expected paragraph")
        };
        let carve::InlineNode::Code(code) = &paragraph.children[0] else {
            panic!("expected code")
        };
        assert_eq!(code.value, value);
    }
}

#[test]
fn native_code_controls_keep_structure_and_neighboring_text() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/markdown-html-code-controls.json")).unwrap();
    for case in cases {
        let source = carve::try_markdown_to_carve(case["markdown"].as_str().unwrap()).unwrap();
        let html = carve::to_html(&source);
        let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
        let mut codes = Vec::new();
        code_records(&dom.document, &[], &mut codes);
        fn roots(node: &Handle, found: &mut Vec<Value>) {
            if let NodeData::Element { name, .. } = &node.data {
                if matches!(name.local.as_ref(), "p" | "h1" | "h2" | "td" | "th") {
                    found.push(json!({"tag": name.local.to_string(), "value": text(node)}));
                }
            }
            for child in node.children.borrow().iter() {
                roots(child, found);
            }
        }
        let mut values = Vec::new();
        roots(&dom.document, &mut values);
        assert_eq!(
            json!({"codes": codes, "roots": values}),
            json!({"codes": case["codes"], "roots": case["roots"]}),
            "{}\n{source}",
            case["markdown"]
        );
    }
}

#[test]
fn raw_code_fallback_is_reported_as_a_rendering_capability_change() {
    let result =
        carve::try_migrate_markdown("| head |\n|---|\n| <code>a<!---->&#10;<!---->b</code> |")
            .unwrap();
    let rows: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|row| row.code == "raw-code-fallback")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].fidelity, carve::MigrationFidelity::Degraded);
    assert_eq!(rows[0].confidence, carve::MigrationConfidence::Exact);
}
