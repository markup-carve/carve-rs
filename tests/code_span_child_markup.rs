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
