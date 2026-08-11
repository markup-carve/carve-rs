use carve::{
    html_to_ast, html_to_carve, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions,
};

#[test]
fn imports_through_the_canonical_writer() {
    let result = html_to_carve(
        "<h1>Hello <em>world</em></h1><p>A <a href=\"https://example.com\">link</a>.</p>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(
        result.value,
        "# Hello /world/\n\nA [link](https://example.com).\n"
    );
    assert!(result.report.diagnostics.is_empty());
}

#[test]
fn active_content_and_loss_are_reported() {
    let result = html_to_ast(
        "<p onclick=\"evil()\">safe<script>alert(1)</script><span title=\"lost\"> text</span></p>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![
            HtmlImportDiagnosticCode::AttributeDropped,
            HtmlImportDiagnosticCode::ElementDropped,
        ]
    );
}

#[test]
fn semantic_mode_keeps_portable_attributes() {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Semantic,
        ..Default::default()
    };
    let result = html_to_carve("<p id=\"lead\" class=\"intro\">Text</p>", &options).unwrap();
    assert!(result.value.contains("{#lead .intro}"));
}

#[test]
fn roundtrip_mode_preserves_unknown_markup_as_raw_html() {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..Default::default()
    };
    let result = html_to_carve("<p><kbd>x</kbd></p>", &options).unwrap();
    assert!(result.value.contains("{=html}"));
    assert_eq!(
        result.report.diagnostics[0].code,
        HtmlImportDiagnosticCode::RawPreserved
    );
}
