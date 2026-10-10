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
        let tag = if name.local.as_ref() == "s" {
            "del".to_string()
        } else {
            name.local.to_string()
        };
        if !matches!(tag.as_str(), "html" | "head" | "body" | "section") {
            next.push(tag.clone());
        }
        if tag == "code" {
            let elements: Vec<_> = node
                .children
                .borrow()
                .iter()
                .filter_map(|child| match &child.data {
                    NodeData::Element { name, .. } => Some(if name.local.as_ref() == "s" {
                        "del".to_string()
                    } else {
                        name.local.to_string()
                    }),
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
        fn attributes(node: &Handle, found: &mut Vec<Value>) {
            if let NodeData::Element { name, attrs, .. } = &node.data {
                if matches!(name.local.as_ref(), "a" | "img") {
                    let mut row = serde_json::Map::new();
                    row.insert("tag".into(), name.local.to_string().into());
                    for attr in attrs.borrow().iter() {
                        if matches!(attr.name.local.as_ref(), "href" | "title" | "src" | "alt") {
                            row.insert(attr.name.local.to_string(), attr.value.to_string().into());
                        }
                    }
                    found.push(Value::Object(row));
                }
            }
            for child in node.children.borrow().iter() {
                attributes(child, found);
            }
        }
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
        let mut attrs = Vec::new();
        attributes(&dom.document, &mut attrs);
        assert_eq!(
            json!({"codes": codes, "roots": values, "attributes": attrs}),
            json!({"codes": case["codes"], "roots": case["roots"], "attributes": case["attributes"]}),
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

#[test]
fn a_footnote_inside_html_code_keeps_its_definition() {
    let source = carve::try_markdown_to_carve("<code>x[^1]</code>\n\n[^1]: note body\n").unwrap();
    let html = carve::to_html(&source);
    assert!(html.contains("note body"), "{source}\n{html}");
    assert!(html.contains("doc-noteref"), "{source}\n{html}");
}

#[test]
fn a_table_fallback_does_not_degrade_a_native_paragraph_code() {
    let result = carve::try_migrate_markdown(
        "<code>a<!---->&#10;<!---->b</code>\n\n| h |\n|---|\n| <code>x&#10;y</code> |\n",
    )
    .unwrap();
    let rows: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|row| row.code == "raw-code-fallback")
        .collect();
    assert_eq!(rows.len(), 1, "{:?}", result.report.diagnostics);
    let document = carve::parse(&result.value);
    let carve::BlockNode::Paragraph(paragraph) = &document.children[0] else {
        panic!("paragraph")
    };
    assert!(
        matches!(&paragraph.children[..], [carve::InlineNode::Code(code)] if code.value == "a\nb")
    );
    let options = carve::Options {
        allow_raw_html: false,
        ..Default::default()
    };
    let html = carve::render_html_with_options(&document, &options).unwrap();
    assert!(html.starts_with("<p><code>a\nb</code></p>"), "{html}");
}

#[test]
fn a_multiline_html_code_loss_names_the_opening_line() {
    let result = carve::try_migrate_markdown("<code>*a*\nb</code>\n").unwrap();
    let rows: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|row| row.code == "raw-code-fallback")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path.as_deref(), Some("line:1"));
}

#[test]
fn nested_and_attributed_html_code_keep_newlines() {
    for markdown in [
        "<code>*a&#10;b*</code>",
        "<code><em>a&#10;b</em></code>",
        "<code>[a&#10;b](u)</code>",
        "<code class=\"x\">a&#10;b</code>",
    ] {
        let result = carve::try_migrate_markdown(markdown).unwrap();
        let html = carve::to_html(&result.value);
        let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
        let mut records = Vec::new();
        code_records(&dom.document, &[], &mut records);
        assert_eq!(records.len(), 1, "{markdown}\n{}", result.value);
        assert_eq!(records[0]["value"], "a\nb", "{markdown}\n{}", result.value);
        assert_eq!(
            result
                .report
                .diagnostics
                .iter()
                .filter(|row| row.code == "raw-code-fallback")
                .count(),
            1,
            "{markdown}"
        );
    }
}

#[test]
fn a_table_fallback_does_not_degrade_a_trailing_empty_code() {
    let result = carve::try_migrate_markdown(
        "text <code></code>\n\n| h |\n|---|\n| <code>x&#10;y</code> |\n",
    )
    .unwrap();
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .filter(|row| row.code == "raw-code-fallback")
            .count(),
        1,
        "{:?}",
        result.report.diagnostics
    );
    let document = carve::parse(&result.value);
    let carve::BlockNode::Paragraph(paragraph) = &document.children[0] else {
        panic!("paragraph")
    };
    assert!(
        matches!(paragraph.children.last(), Some(carve::InlineNode::Code(code)) if code.value.is_empty()),
        "{}",
        result.value
    );
}

#[test]
fn a_table_fallback_does_not_degrade_an_empty_heading_code() {
    let result =
        carve::try_migrate_markdown("# <code></code>\n\n| h |\n|---|\n| <code>x&#10;y</code> |\n")
            .unwrap();
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .filter(|row| row.code == "raw-code-fallback")
            .count(),
        1,
        "{:?}",
        result.report.diagnostics
    );
    let document = carve::parse(&result.value);
    let carve::BlockNode::Heading(heading) = &document.children[0] else {
        panic!("heading")
    };
    assert!(
        matches!(heading.children.last(), Some(carve::InlineNode::Code(code)) if code.value.is_empty()),
        "{}",
        result.value
    );
}
