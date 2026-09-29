//! Literal code payloads, including their physical line endings (CARVE-P12-064).

use carve::{parse, to_json, BlockNode, Options};
use serde_json::Value;

fn contents(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("code_block") {
                out.push(object["content"].as_str().unwrap().to_owned());
            }
            for child in object.values() {
                contents(child, out);
            }
        }
        Value::Array(values) => {
            for child in values {
                contents(child, out);
            }
        }
        _ => {}
    }
}

fn payloads(source: &str, options: &Options<'_>) -> Vec<String> {
    let value: Value =
        serde_json::from_str(&to_json(&carve::parse_with_options(source, options))).unwrap();
    let mut out = Vec::new();
    contents(&value, &mut out);
    out
}

#[test]
fn shared_corpus_content_fixtures() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "spec/resources/ast-code-content-fixtures.json"
    ))
    .unwrap();
    for (name, expected) in fixtures.as_object().unwrap() {
        let source = std::fs::read_to_string(format!(
            "{}/tests/spec/tests/corpus/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        assert_eq!(
            serde_json::json!(payloads(&source, &Options::default())),
            *expected,
            "{name}"
        );
    }
}

#[test]
fn shared_exact_eof_samples() {
    let fixtures: Value =
        serde_json::from_str(include_str!("spec/resources/ast-code-payload-samples.json")).unwrap();
    for sample in fixtures.as_array().unwrap() {
        let source = sample["source"].as_str().unwrap();
        for positions in [false, true] {
            let actual = payloads(source, &Options::default().with_positions(positions));
            assert_eq!(
                actual,
                [sample["content"].as_str().unwrap()],
                "{} positions={positions}",
                sample["name"]
            );
        }
    }
}

#[test]
fn containers_keep_the_physical_eof() {
    for source in [
        "```\na",
        "> ```\n> a",
        "- ```\n  a",
        "::: note\n```\na",
        ":: term\n: ```\n  a",
        "[^n]: ```\n  a",
        "[r]: /url\n\n```\na",
        "---\ntitle: t\n---\n```\na",
        "> - ```\n>   a",
        "- > ```\n  > a",
        "::: note\n::: note\n```\na",
    ] {
        for ending in ["", "\n", "\r\n", "\r"] {
            for positions in [false, true] {
                let input = format!("{source}{ending}");
                let actual = payloads(&input, &Options::default().with_positions(positions));
                let expected = if ending.is_empty() { "a" } else { "a\n" };
                assert_eq!(actual, [expected], "{input:?} positions={positions}");
            }
        }
    }
}

#[test]
fn imported_and_constructed_content_round_trips_literally() {
    for content in ["", "a", "a\n", "a\n\n", "\n", "\n\n"] {
        let input = serde_json::json!({"type":"document","srcByteLength":0,"children":[{"type":"code_block","content":content}]}).to_string();
        let doc = carve::from_json(&input).unwrap();
        let html = format!("<pre><code>{content}</code></pre>");
        assert_eq!(carve::render_html(&doc).unwrap().trim_end(), html);
        let imported = carve::html_to_ast(&html, &Default::default()).unwrap();
        assert_eq!(payload_from_doc(&imported.value), content);
        let report = carve::conversion_diagnostics(&doc, 100).unwrap();
        let needs_ending = !content.is_empty() && !content.ends_with('\n');
        assert_eq!(report.total_diagnostics, usize::from(needs_ending));
        if needs_ending {
            assert_eq!(
                report.diagnostics[0].code,
                carve::ConversionDiagnosticCode::FieldUnspellable
            );
            assert_eq!(report.diagnostics[0].node, "code_block");
            assert_eq!(report.diagnostics[0].field.as_deref(), Some("content"));
        }
        let written = carve::render_carve(&doc).unwrap();
        let expected = if needs_ending {
            format!("{content}\n")
        } else {
            content.to_owned()
        };
        assert_eq!(payload_from_doc(&parse(&written)), expected);
    }
}

fn payload_from_doc(doc: &carve::Document) -> &str {
    let BlockNode::CodeBlock(code) = &doc.children[0] else {
        panic!("missing code block")
    };
    &code.content
}

#[test]
fn markdown_import_keeps_the_payload_break() {
    for (source, expected) in [("```\na\n```", "a\n"), ("    a\n", "a\n"), ("```\na", "a")] {
        let doc = carve::markdown_to_ast(source);
        assert_eq!(payload_from_doc(&doc), expected, "{source:?}");
    }
}

#[test]
fn diagnostic_caps_count_each_unterminated_payload() {
    let doc = carve::from_json(r#"{"type":"document","srcByteLength":0,"children":[{"type":"block_quote","children":[{"type":"code_block","content":"a"},{"type":"code_block","content":"b"}]}]}"#).unwrap();
    for cap in [0, 1, 2] {
        let report = carve::conversion_diagnostics(&doc, cap).unwrap();
        assert_eq!(report.total_diagnostics, 2);
        assert_eq!(report.diagnostics.len(), cap);
        assert_eq!(report.truncated, cap < 2);
    }
}

struct FragmentHost;

impl carve::CarveExtension for FragmentHost {
    fn name(&self) -> &'static str {
        "fragment-host"
    }

    fn match_block(
        &self,
        lines: &[&str],
        start: usize,
        ctx: &carve::MatcherContext<'_>,
    ) -> Option<carve::BlockMatch> {
        if lines.get(start) != Some(&"@@@") {
            return None;
        }
        let end = start + 1 + lines[start + 1..].iter().position(|line| *line == "@@@")?;
        Some(carve::BlockMatch {
            node: BlockNode::Div(carve::Div {
                attrs: None,
                label: None,
                children: ctx.parse_blocks(&lines[start + 1..end].join("\n")),
                pos: None,
            }),
            lines_consumed: end - start + 1,
        })
    }
}

#[test]
fn extension_fragments_do_not_claim_the_documents_eof() {
    for positions in [false, true] {
        let options = Options::default()
            .with_extension(&FragmentHost)
            .with_positions(positions);
        for source in [
            "@@@\n```\na\n@@@\n",
            "@@@\n```\na\n@@@",
            "@@@\n```\na\n@@@\n\n```\nb",
        ] {
            let doc = carve::parse_with_options(source, &options);
            let mut actual = Vec::new();
            contents(&serde_json::from_str(&to_json(&doc)).unwrap(), &mut actual);
            let expected = if source.ends_with('b') {
                vec!["a\n", "b"]
            } else {
                vec!["a\n"]
            };
            assert_eq!(actual, expected, "{source:?} positions={positions}");
            assert_eq!(
                carve::conversion_diagnostics(&doc, 100)
                    .unwrap()
                    .total_diagnostics,
                usize::from(source.ends_with('b'))
            );
        }
    }
}
