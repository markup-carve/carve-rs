use carve::{html_to_carve, to_carve, to_html, HtmlImportDiagnosticCode, HtmlImportOptions};

#[test]
fn child_markup_losses_are_reported() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/code-span-child-markup.json")).unwrap();
    for case in cases {
        let result = html_to_carve(
            case["html"].as_str().unwrap(),
            &HtmlImportOptions::default(),
        )
        .unwrap();
        let loss = result.report.diagnostics.iter().any(|d| {
            matches!(
                d.code,
                HtmlImportDiagnosticCode::ElementUnwrapped
                    | HtmlImportDiagnosticCode::ElementDropped
            ) && d
                .path
                .as_deref()
                .is_some_and(|p| p.starts_with("/p[1]/code[1]/"))
        });
        assert_eq!(loss, case["loss"].as_bool().unwrap(), "{}", case["name"]);
        assert_eq!(to_html(&result.value).trim(), "<p><code>word</code></p>");
        assert_eq!(to_carve(&result.value), result.value);
    }
}

#[test]
fn a_code_span_line_break_in_a_pipe_cell_is_reported() {
    let result = html_to_carve(
        "<table><tr><td><code>x\ny</code></td></tr></table>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert!(result
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == HtmlImportDiagnosticCode::StructureUnspellable));
    assert!(to_html(&result.value).contains("<td><code>x y</code></td>"));
    assert_eq!(to_carve(&result.value), result.value);
}

#[test]
fn every_mode_reports_discarded_children_without_preservation_claims() {
    use carve::{html_to_ast, HtmlImportMode};
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        let opts = HtmlImportOptions {
            mode,
            ..Default::default()
        };
        let html = "<p><code><span><strong class=\"k\">word</strong></span></code></p>";
        let ast = html_to_ast(html, &opts).unwrap();
        let source = html_to_carve(html, &opts).unwrap();
        for report in [&ast.report, &source.report] {
            let rows: Vec<_> = report
                .diagnostics
                .iter()
                .map(|d| (d.code, d.path.as_deref()))
                .collect();
            assert_eq!(
                rows,
                vec![
                    (
                        HtmlImportDiagnosticCode::ElementUnwrapped,
                        Some("/p[1]/code[1]/span[1]/strong[1]")
                    ),
                    (
                        HtmlImportDiagnosticCode::AttributeDropped,
                        Some("/p[1]/code[1]/span[1]/strong[1]")
                    ),
                ]
            );
        }
        for tag in ["q", "math", "ruby", "summary", "code", "unknown"] {
            let result =
                html_to_carve(&format!("<code><{tag}>word</{tag}></code>"), &opts).unwrap();
            assert_eq!(result.value, "`word`\n");
            assert_eq!(
                result
                    .report
                    .diagnostics
                    .iter()
                    .map(|d| d.code)
                    .collect::<Vec<_>>(),
                vec![HtmlImportDiagnosticCode::ElementUnwrapped]
            );
        }
        for tag in ["br", "input", "img"] {
            let result = html_to_carve(&format!("<code><{tag}>word</code>"), &opts).unwrap();
            assert_eq!(result.value, "`word`\n");
            assert_eq!(
                result
                    .report
                    .diagnostics
                    .iter()
                    .map(|d| d.code)
                    .collect::<Vec<_>>(),
                vec![HtmlImportDiagnosticCode::ElementDropped]
            );
        }
        let result = html_to_carve("<code>a<script>b</script><!--c-->d</code>", &opts).unwrap();
        assert_eq!(result.value, "`ad`\n");
        assert_eq!(
            result
                .report
                .diagnostics
                .iter()
                .map(|d| (d.code, d.path.as_deref()))
                .collect::<Vec<_>>(),
            vec![
                (
                    HtmlImportDiagnosticCode::ElementDropped,
                    Some("/code[1]/script[2]")
                ),
                (
                    HtmlImportDiagnosticCode::ElementDropped,
                    Some("/code[1]/comment()[3]")
                ),
            ]
        );
        let result = html_to_carve("<code><b><div>a</div>b</b></code>", &opts).unwrap();
        assert_eq!(result.value, "`ab`\n");
        assert_eq!(
            result
                .report
                .diagnostics
                .iter()
                .filter(|d| d.code == HtmlImportDiagnosticCode::StructureUnspellable)
                .count(),
            1
        );
    }
}

#[test]
fn discarded_active_descendants_are_charged_to_the_budget() {
    use carve::html_to_ast;
    let html = "<p><code><script>x</script></code></p>";
    for max_nodes in [3, 4] {
        let opts = HtmlImportOptions {
            max_nodes,
            ..Default::default()
        };
        assert_eq!(html_to_ast(html, &opts).is_ok(), max_nodes == 4);
        assert_eq!(html_to_carve(html, &opts).is_ok(), max_nodes == 4);
    }
}

#[test]
fn ast_table_code_preserves_line_breaks() {
    let result = carve::html_to_ast(
        "<table><tr><td><code>x\ny</code></td></tr></table>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert!(result.report.diagnostics.is_empty());
    let carve::BlockNode::Table(table) = &result.value.children[0] else {
        panic!("expected table");
    };
    let carve::InlineNode::Code(code) = &table.rows[0].cells[0].children[0] else {
        panic!("expected code span");
    };
    assert_eq!(code.value, "x\ny");
}

#[test]
fn footnote_looking_code_text_does_not_consume_an_endnote() {
    let html = "<p><code>x<sup><a href=\"#fn1\" role=\"doc-noteref\">1</a></sup></code></p><section role=\"doc-endnotes\"><ol><li id=\"fn1\"><p>note</p></li></ol></section>";
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    let rendered = to_html(&result.value);
    assert!(rendered.contains("<code>x1</code>"));
    assert!(rendered.contains("note"));
}

#[test]
fn template_content_is_charged_to_both_import_limits() {
    let html = "<p><code><template><b>x</b></template>word</code></p>";
    let low_nodes = HtmlImportOptions {
        max_nodes: 5,
        ..Default::default()
    };
    let low_depth = HtmlImportOptions {
        max_depth: 4,
        ..Default::default()
    };
    let enough = HtmlImportOptions {
        max_nodes: 6,
        max_depth: 5,
        ..Default::default()
    };
    assert!(carve::html_to_ast(html, &low_nodes).is_err());
    assert!(carve::html_to_ast(html, &low_depth).is_err());
    assert!(carve::html_to_ast(html, &enough).is_ok());
}
