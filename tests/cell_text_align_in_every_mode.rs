use carve::{
    html_to_ast, html_to_carve, render_carve, HtmlImportDiagnosticCode, HtmlImportMode,
    HtmlImportOptions,
};

#[test]
fn cell_text_align_in_every_mode() {
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        let options = HtmlImportOptions {
            mode,
            ..Default::default()
        };
        for (html, source, count) in [
            ("<table><thead><tr><th style=\"text-align:left\">V</th><th style=\"text-align:right\">D</th></tr></thead><tbody><tr><td style=\"text-align:left\">a</td><td style=\"text-align:right\">b</td></tr></tbody></table>", "|=< V |=> D |\n| a | b |\n", 0),
            ("<table><tr><th style=\"text-align:right\">V</th></tr><tr><td style=\"text-align:center\">a</td></tr><tr><td style=\"text-align:left\">b</td></tr></table>", "|=> V |\n|~ a |\n|< b |\n", 0),
            ("<table><tr><td style=\"text-align:justify;text-align:right\">a</td><td style=\"text-align:right;text-align:left\">b</td><td style=\"text-align:right !important\">c</td><td style=\"text-align:center;text-align:justify\">d</td></tr></table>", "|> a |< b | c |~ d |\n", 3),
            ("<table><tr><td style=\"TEXT-ALIGN: CENTER; color:red\">a</td><td style=\"text-align:justify\">b</td></tr></table>", "|~ a | b |\n", 2),
        ] {
            let result = html_to_carve(html, &options).unwrap();
            assert_eq!(result.value, source);
            let ast = html_to_ast(html, &options).unwrap();
            assert_eq!(render_carve(&ast.value).unwrap(), source);
            assert_eq!(ast.report.diagnostics.len(), count);
            assert!(ast.report.diagnostics.iter().all(|d| d.code == HtmlImportDiagnosticCode::StyleUnmapped));
            assert_eq!(result.report.diagnostics.len(), count);
            assert!(result.report.diagnostics.iter().all(|d| d.code == HtmlImportDiagnosticCode::StyleUnmapped));
        }
    }
}
