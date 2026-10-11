use carve::{ast_json, parse, to_html_with_options, BlockNode, Options};

fn payloads(value: &serde_json::Value, kind: &str, result: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(object) => {
            if object.get("type").and_then(serde_json::Value::as_str) == Some(kind) {
                result.push(object["content"].as_str().unwrap().to_owned());
            }
            for value in object.values() {
                payloads(value, kind, result);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                payloads(value, kind, result);
            }
        }
        _ => {}
    }
}

#[test]
fn shared_ast_vectors_keep_literal_payloads_in_safe_and_matching_targets() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/raw-payload-termination.json")).unwrap();
    for vector in vectors.as_array().unwrap() {
        let wire =
            serde_json::json!({"type":"document", "srcByteLength":0, "children":[vector["raw"]]});
        let doc = ast_json::from_json(&wire.to_string()).unwrap();
        let back: serde_json::Value = serde_json::from_str(&ast_json::to_json(&doc)).unwrap();
        assert_eq!(back, wire, "{}", vector["id"]);
        let code = ast_json::from_json(
            &serde_json::json!({
                "type":"document", "srcByteLength":0, "children":[{
                    "type":"code_block", "lang":"html", "content":vector["codeContent"]
                }]
            })
            .to_string(),
        )
        .unwrap();
        let safe = Options {
            allow_raw_html: false,
            ..Options::default()
        };
        assert_eq!(
            carve::render_html_with_options(&doc, &safe).unwrap(),
            carve::render_html_with_options(&code, &safe).unwrap(),
            "{}",
            vector["id"]
        );
        let matching = Options {
            allow_raw_html: true,
            ..Options::default()
        };
        let target = match vector["id"].as_str().unwrap() {
            "empty" => "",
            "blank-one" => "\n",
            "blank-two" => "\n\n",
            "blank-three" => "\n\n\n",
            "text-eof" | "text-terminated" => "x",
            "text-blank-one" => "x\n",
            "text-blank-two" => "x\n\n",
            "space-tab-eof" | "space-tab-terminated" => " \t",
            "nbsp-eof" | "nbsp-terminated" => "\u{a0}",
            id => panic!("unknown vector {id}"),
        };
        assert_eq!(
            carve::render_html_with_options(&doc, &matching).unwrap(),
            target,
            "{}",
            vector["id"]
        );
        let expected_ansi_payload = if vector["id"] == "blank-three" {
            "\n\n"
        } else {
            target
        };
        assert_eq!(
            carve::render_ansi(&doc).unwrap(),
            format!("\x1b[2m[raw:html] {expected_ansi_payload}\x1b[0m\n"),
            "{}",
            vector["id"]
        );
        let expected_markdown = match vector["id"].as_str().unwrap() {
            "empty" | "blank-one" | "blank-two" | "blank-three" => "\n",
            "text-eof" | "text-terminated" | "text-blank-one" | "text-blank-two" => "x\n",
            "space-tab-eof" | "space-tab-terminated" => "\t\n",
            "nbsp-eof" | "nbsp-terminated" => "\u{a0}\n",
            _ => unreachable!(),
        };
        assert_eq!(
            carve::render_markdown(&doc).unwrap(),
            expected_markdown,
            "{}",
            vector["id"]
        );
        let profile = carve::Profile::default().deny_block(&["raw_block", "code_block"]);
        let filtered = carve::apply_profile(doc.clone(), &profile, None).unwrap();
        let BlockNode::Paragraph(paragraph) = &filtered.doc.children[0] else {
            panic!("expected paragraph")
        };
        let text: String = paragraph
            .children
            .iter()
            .map(|node| match node {
                carve::InlineNode::Text(text) => text.value.clone(),
                carve::InlineNode::HardBreak(_) => "\n".to_owned(),
                _ => panic!("unexpected paragraph node {node:?}"),
            })
            .collect();
        assert_eq!(
            text,
            if target.is_empty() {
                "[raw_block]"
            } else {
                target
            },
            "{}",
            vector["id"]
        );
    }
}

#[test]
fn source_eof_retains_actual_payload_like_code_for_each_raw_format() {
    let safe = Options {
        allow_raw_html: false,
        ..Options::default()
    };
    for format in ["html", "latex", "markdown"] {
        for payload in ["", "\n", "\n\n", "x", "x\n", "x\n\n", " \t", "\u{a0}"] {
            let source = format!("```={format}\n{payload}");
            let doc = parse(&source);
            let BlockNode::RawBlock(raw) = &doc.children[0] else {
                panic!("missing raw")
            };
            assert_eq!(raw.content, payload, "{format}: {payload:?}");
            if format == "html" {
                assert_eq!(
                    to_html_with_options(&source, &safe),
                    to_html_with_options(&format!("```html\n{payload}"), &safe)
                );
            }
        }
    }
}

#[test]
fn shared_source_hosts_preserve_safe_profile_and_ast_payloads() {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/raw-payload-source-fixtures.json")).unwrap();
    for fixture in fixtures.as_array().unwrap() {
        let source = fixture["source"].as_str().unwrap();
        let code_source = fixture["codeSource"].as_str().unwrap();
        for options in [
            Options {
                allow_raw_html: false,
                ..Options::default()
            },
            Options {
                profile: Some(carve::Profile::article()),
                ..Options::default()
            },
        ] {
            assert_eq!(
                to_html_with_options(source, &options),
                to_html_with_options(code_source, &options),
                "{}",
                fixture["id"]
            );
        }
        let raw: serde_json::Value =
            serde_json::from_str(&ast_json::to_json(&parse(source))).unwrap();
        let code: serde_json::Value =
            serde_json::from_str(&ast_json::to_json(&parse(code_source))).unwrap();
        let (mut raw_payloads, mut code_payloads) = (Vec::new(), Vec::new());
        payloads(&raw, "raw_block", &mut raw_payloads);
        payloads(&code, "code_block", &mut code_payloads);
        assert!(!raw_payloads.is_empty(), "{}", fixture["id"]);
        assert_eq!(raw_payloads, code_payloads, "{}", fixture["id"]);
        assert!(!raw.to_string().contains("contentTerminated"));
    }
}

#[test]
fn formatter_reports_only_unterminated_nonempty_content() {
    for payload in ["", "\n", "x", "x\n", "x\n\n"] {
        let doc = parse(&format!("```=html\n{payload}"));
        let report = carve::conversion_diagnostics::conversion_diagnostics(&doc, 100).unwrap();
        if !payload.is_empty() && !payload.ends_with('\n') {
            assert_eq!(report.diagnostics.len(), 1);
            assert_eq!(report.diagnostics[0].field.as_deref(), Some("content"));
        } else {
            assert!(report.diagnostics.is_empty(), "{payload:?}");
        }
    }
}

#[test]
fn typed_wire_refuses_the_removed_termination_flag() {
    for value in [
        serde_json::Value::Bool(false),
        serde_json::Value::Bool(true),
        serde_json::Value::Null,
    ] {
        let wire = serde_json::json!({"type":"document", "srcByteLength":0, "children":[{
            "type":"raw_block", "format":"html", "content":"x", "contentTerminated":value
        }]});
        let error = ast_json::from_json(&wire.to_string()).unwrap_err();
        assert_eq!(error.kind(), ast_json::AstJsonErrorKind::UnknownField);
    }
}

#[test]
fn prosemirror_roundtrip_keeps_literal_termination() {
    for payload in ["", "\n", "x", "x\n", "x\n\n"] {
        let doc = parse(&format!("```=html\n{payload}"));
        let pm = carve::to_prosemirror(&doc);
        let back = carve::from_prosemirror(&pm.json).unwrap();
        let BlockNode::RawBlock(raw) = &back.children[0] else {
            panic!("missing raw")
        };
        assert_eq!(raw.content, payload);
    }
}

#[test]
fn marker_only_eof_uses_literal_payload_without_termination_metadata() {
    for (source, content, equivalent) in [
        ("> ```=html\n> ", "", "> ```=html\n> ```\n"),
        ("> ```=html\n> x\n> ", "x\n", "> ```=html\n> x\n> ```\n"),
        ("> ```=html\n> \n> ", "\n", "> ```=html\n> \n> ```\n"),
    ] {
        let wire: serde_json::Value =
            serde_json::from_str(&ast_json::to_json(&parse(source))).unwrap();
        let mut actual = Vec::new();
        payloads(&wire, "raw_block", &mut actual);
        assert_eq!(actual, [content]);
        assert!(!wire.to_string().contains("contentTerminated"));
        assert_eq!(carve::to_html(source), carve::to_html(equivalent));
    }
}

#[test]
fn closed_and_container_ended_payloads_match_code() {
    for (raw, code) in [
        ("> ```=html\n> x\npara\n", "> ```html\n> x\npara\n"),
        ("> ```=html\n> x\n> ```\n", "> ```html\n> x\n> ```\n"),
        ("- ```=html\n  x\n  ```\n", "- ```html\n  x\n  ```\n"),
        (
            "::: box\n```=html\nx\n```\n:::\n",
            "::: box\n```html\nx\n```\n:::\n",
        ),
    ] {
        let mut raw_payloads = Vec::new();
        let mut code_payloads = Vec::new();
        payloads(
            &serde_json::from_str(&ast_json::to_json(&parse(raw))).unwrap(),
            "raw_block",
            &mut raw_payloads,
        );
        payloads(
            &serde_json::from_str(&ast_json::to_json(&parse(code))).unwrap(),
            "code_block",
            &mut code_payloads,
        );
        assert!(!raw_payloads.is_empty());
        assert_eq!(raw_payloads, code_payloads, "{raw:?}");
    }
}

#[test]
fn source_writer_retains_terminated_payloads_and_reports_added_ending() {
    for content in ["", "\n", "\n\n", "x", "x\n", "x\n\n", " \t", "\u{a0}"] {
        let wire = serde_json::json!({"type":"document", "srcByteLength":0,"children":[{"type":"raw_block","format":"html","content":content}]});
        let doc = ast_json::from_json(&wire.to_string()).unwrap();
        let written = carve::render_carve(&doc).unwrap();
        let mut actual = Vec::new();
        payloads(
            &serde_json::from_str(&ast_json::to_json(&parse(&written))).unwrap(),
            "raw_block",
            &mut actual,
        );
        let needs_ending = !content.is_empty() && !content.ends_with('\n');
        assert_eq!(
            actual,
            [if needs_ending {
                format!("{content}\n")
            } else {
                content.to_owned()
            }]
        );
        let report = carve::conversion_diagnostics::conversion_diagnostics(&doc, 100).unwrap();
        if needs_ending {
            assert_eq!(report.diagnostics.len(), 1);
            assert_eq!(
                report.diagnostics[0].code,
                carve::conversion_diagnostics::ConversionDiagnosticCode::FieldUnspellable
            );
            assert_eq!(report.diagnostics[0].field.as_deref(), Some("content"));
        } else {
            assert!(report.diagnostics.is_empty());
        }
    }
}
