use carve::{
    html_to_ast, html_to_carve, to_carve, to_html, BlockNode, HtmlImportDiagnosticCode,
    HtmlImportMode, HtmlImportOptions,
};

#[test]
fn code_payload_and_loss_reports_agree_in_every_mode() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/code-block-child-markup.json")).unwrap();
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        let opts = HtmlImportOptions {
            mode,
            ..Default::default()
        };
        for case in &cases {
            let input = case["html"].as_str().unwrap();
            let ast = html_to_ast(input, &opts).unwrap();
            let source = html_to_carve(input, &opts).unwrap();
            let BlockNode::CodeBlock(code) = &ast.value.children[0] else {
                panic!("not a code block: {}", case["name"]);
            };
            assert_eq!(
                code.content,
                case["content"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            for report in [&ast.report, &source.report] {
                let codes: Vec<_> = report
                    .diagnostics
                    .iter()
                    .map(|d| match d.code {
                        HtmlImportDiagnosticCode::ElementDropped => "element-dropped",
                        HtmlImportDiagnosticCode::ElementUnwrapped => "element-unwrapped",
                        HtmlImportDiagnosticCode::AttributeDropped => "attribute-dropped",
                        _ => panic!("unexpected diagnostic: {:?}", d.code),
                    })
                    .collect();
                assert_eq!(serde_json::json!(codes), case["codes"], "{}", case["name"]);
            }
            assert!(
                to_html(&source.value).contains("<p>after</p>"),
                "{}",
                case["name"]
            );
            assert_eq!(to_carve(&source.value), source.value, "{}", case["name"]);
            let html = to_html(&source.value);
            let reparsed = html_to_ast(&html, &opts).unwrap();
            let BlockNode::CodeBlock(code) = &reparsed.value.children[0] else {
                panic!("not a code block");
            };
            let content = case["content"].as_str().unwrap();
            let expected = if !content.is_empty() && !content.ends_with('\n') {
                format!("{content}\n")
            } else {
                content.into()
            };
            assert_eq!(code.content, expected, "{}", case["name"]);
        }
    }
}

#[test]
fn nested_code_markup_is_counted_toward_import_limits() {
    let input = format!("<pre><code>{}</code></pre>", "<span>x</span>".repeat(100));
    assert!(html_to_ast(
        &input,
        &HtmlImportOptions {
            max_nodes: 10,
            ..Default::default()
        }
    )
    .is_err());
}
