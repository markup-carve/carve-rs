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
        let result = carve::try_migrate_markdown(case["markdown"].as_str().unwrap()).unwrap();
        let source = &result.value;
        let html = carve::to_html(source);
        let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
        let mut records = Vec::new();
        code_records(&dom.document, &[], &mut records);
        let value = case["value"].as_str().unwrap();
        if !value.is_empty() && !value.contains(['\r', '\n']) {
            assert!(!result
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "raw-code-fallback"));
            let options = carve::Options {
                allow_raw_html: false,
                ..Default::default()
            };
            let html = carve::render_html_with_options(&carve::parse(source), &options).unwrap();
            let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
            let mut safe_records = Vec::new();
            code_records(&dom.document, &[], &mut safe_records);
            assert_eq!(safe_records, records);
        }
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
    assert_eq!(rows[0].path.as_deref(), Some("line:3"));
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

#[test]
fn an_empty_code_in_a_link_does_not_degrade_other_code() {
    for markdown in [
        "`keep`\n\n[a<code></code>](u)\n",
        "`keep`\n\n[<code></code>](u)\n",
    ] {
        let result = carve::try_migrate_markdown(markdown).unwrap();
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
            matches!(&paragraph.children[..], [carve::InlineNode::Code(code)] if code.value == "keep"),
            "{}",
            result.value
        );
    }
}

#[test]
fn text_after_a_misnested_code_close_survives_raw_html_stripping() {
    let source = carve::try_markdown_to_carve("<code>*a</code> b*").unwrap();
    let options = carve::Options {
        allow_raw_html: false,
        ..Default::default()
    };
    let html = carve::render_html_with_options(&carve::parse(&source), &options).unwrap();
    assert!(html.contains(" b"), "{source}\n{html}");
}

#[test]
fn repeated_empty_comments_keep_code_text() {
    let markdown = format!("<code>{}</code>", "a<!---->".repeat(20_000));
    let source = carve::try_markdown_to_carve(&markdown).unwrap();
    assert_eq!(
        carve::to_html(&source),
        format!("<p><code>{}</code></p>", "a".repeat(20_000))
    );
}

#[test]
fn code_opening_whitespace_loss_keeps_its_source_line() {
    let result = carve::try_migrate_markdown("<code \nclass=\"x\">a\nb\nc</code>").unwrap();
    let rows: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|row| row.code == "raw-span-whitespace-trimmed")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path.as_deref(), Some("line:1"));
}

#[test]
fn unrelated_native_code_survives_single_line_writer_contexts() {
    for markdown in ["`keep`\n\n$`x`", "`keep`\n\n[^u]: `b`"] {
        let source = carve::try_markdown_to_carve(markdown).unwrap();
        assert!(!source.contains("{=html}"), "{markdown}\n{source}");
    }
}

#[test]
fn misnested_code_close_restores_prose_newlines() {
    for (markdown, expected) in [
        ("<code>*a</code> b&#10;&#10;c*", " b  c"),
        ("<code>*a</code> b&#13;c*", " b c"),
    ] {
        let source = carve::try_markdown_to_carve(markdown).unwrap();
        assert!(
            carve::to_html(&source).contains(expected),
            "{markdown}\n{source}\n{}",
            carve::to_html(&source)
        );
    }
}

#[test]
fn payload_fallbacks_keep_unrelated_code_native() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/markdown-html-code-payloads.json")).unwrap();
    let options = carve::Options {
        allow_raw_html: false,
        ..Default::default()
    };
    for case in cases {
        let markdown = format!("{}\n\n`keep`", case["markdown"].as_str().unwrap());
        let result = carve::try_migrate_markdown(&markdown).unwrap();
        let document = carve::parse(&result.value);
        let html = carve::render_html_with_options(&document, &options).unwrap();
        assert!(html.contains("<code>keep</code>"), "{}", case["template"]);
        assert!(
            result
                .report
                .diagnostics
                .iter()
                .filter(|row| row.code == "raw-code-fallback")
                .count()
                <= 1,
            "{}",
            case["template"]
        );
    }
}

#[test]
fn a_label_fallback_keeps_its_source_line() {
    let result = carve::try_migrate_markdown("text\n\n[<code></code>](u)").unwrap();
    let rows: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|row| row.code == "raw-code-fallback")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path.as_deref(), Some("line:3"));
}

#[test]
fn nested_html_code_does_not_consume_ast_depth() {
    for closed in [false, true] {
        let depth = 1000;
        let markdown = format!(
            "{}x{}",
            "<code>".repeat(depth),
            if closed {
                "</code>".repeat(depth)
            } else {
                String::new()
            }
        );
        let result = carve::try_migrate_markdown(&markdown).unwrap();
        assert_eq!(
            result
                .report
                .diagnostics
                .iter()
                .filter(|row| row.code == "raw-code-fallback")
                .count(),
            1
        );
        let html = carve::to_html(&result.value);
        assert!(
            html.trim_end() == format!("<p>{markdown}</p>"),
            "nested code changed its HTML"
        );
    }
}

#[test]
fn additional_writer_contexts_keep_unrelated_code_native() {
    let options = carve::Options {
        allow_raw_html: false,
        ..Default::default()
    };
    for markdown in [
        "`keep`\n\nx[^1]\n\n[^1]: <code>a&#10;b</code>\n",
        "`keep`\n\n<ins><code></code></ins> y\n",
        "`keep`\n\n<sup><code>a&#10;b</code></sup> y\n",
        "`keep`\n\n~~<code>a&#10;b</code>~~ y\n",
    ] {
        let result = carve::try_migrate_markdown(markdown).unwrap();
        let html = carve::render_html_with_options(&carve::parse(&result.value), &options).unwrap();
        assert!(html.contains("<code>keep</code>"), "{markdown}");
    }
}

#[test]
fn formatting_outside_code_keeps_its_structure() {
    for (markdown, expected) in [
        (
            "<em><strong>x</strong></em>",
            "<p><em><strong>x</strong></em></p>",
        ),
        (
            "<b>x<sup>2</sup></b>",
            "<p><strong>x<sup>2</sup></strong></p>",
        ),
        ("<em>H<sub>2</sub>O</em>", "<p><em>H<sub>2</sub>O</em></p>"),
        (
            "<strong><del>x</del></strong>",
            "<p><strong><del>x</del></strong></p>",
        ),
    ] {
        let source = carve::try_markdown_to_carve(markdown).unwrap();
        let html = carve::to_html(&source)
            .replace("<s>", "<del>")
            .replace("</s>", "</del>");
        assert_eq!(html.trim_end(), expected);
    }
}

#[test]
fn misnested_code_keeps_entity_newlines_until_its_html_close() {
    for (entity, expected) in [
        ("&#10;", " z\nw"),
        ("&#13;", " z\nw"),
        ("&#13;&#10;", " z\nw"),
        ("&#13;<!---->&#10;", " z\n\nw"),
    ] {
        for template in [
            "*x <code>y* zENTITYw</code> tail&#10;end",
            "[x <code>y](u) zENTITYw</code> tail&#10;end",
            "# *x <code>y* zENTITYw</code> tail&#10;end",
            "| *x <code>y* zENTITYw</code> tail&#10;end |\n| --- |",
            "- *x <code>y* zENTITYw</code> tail&#10;end",
        ] {
            let markdown = template.replace("ENTITY", entity);
            let migration = carve::try_migrate_markdown(&markdown).unwrap();
            let ast = carve::try_markdown_to_ast(&markdown).unwrap();
            for html in [
                carve::to_html(&migration.value),
                carve::render_html(&ast).unwrap(),
            ] {
                let dom = html5ever::parse_document(RcDom::default(), Default::default())
                    .one(html.clone());
                let mut records = Vec::new();
                code_records(&dom.document, &[], &mut records);
                assert_eq!(
                    records
                        .iter()
                        .map(|record| record["value"].as_str().unwrap())
                        .collect::<Vec<_>>(),
                    vec!["y", expected],
                    "{markdown}"
                );
                assert!(
                    html.contains(" tail end"),
                    "prose outside code changed: {markdown}"
                );
            }
            let rows: Vec<_> = migration
                .report
                .diagnostics
                .iter()
                .filter(|row| row.code == "raw-code-fallback")
                .collect();
            assert_eq!(rows.len(), 1, "{markdown}");
            assert_eq!(rows[0].path.as_deref(), Some("line:1"));
            assert_eq!(rows[0].fidelity.as_str(), "degraded");
            assert_eq!(rows[0].confidence.as_str(), "exact");
        }
    }
}

#[test]
fn unclosed_code_does_not_change_later_tight_item_text() {
    for markdown in [
        "- *x <code>y*
- a&#10;b
- c&#13;<!---->&#10;d",
        "- *x <code>y*
  - a&#10;b
  - c&#13;<!---->&#10;d",
    ] {
        let source = carve::try_markdown_to_carve(markdown).unwrap();
        let ast = carve::try_markdown_to_ast(markdown).unwrap();
        for html in [carve::to_html(&source), carve::render_html(&ast).unwrap()] {
            assert!(
                html.contains("a b"),
                "later item changed: {markdown}: {html}"
            );
            assert!(
                html.contains("c <!----> d"),
                "later comment changed: {markdown}: {html}"
            );
        }
    }
}
