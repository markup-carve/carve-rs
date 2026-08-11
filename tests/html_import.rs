use carve::{
    html_to_ast, html_to_carve, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions,
};
use std::fs;
use std::path::Path;

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
    assert!(result.value.contains("`<kbd>x</kbd>`{=html}"));
    assert_eq!(
        result.report.diagnostics[0].code,
        HtmlImportDiagnosticCode::RawPreserved
    );
}

#[test]
fn shared_contract_fixtures_match() {
    let root = Path::new("tests/spec/tests/html-import");
    for entry in fs::read_dir(root).unwrap() {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        let html = fs::read_to_string(dir.join("input.html")).unwrap();
        let expected = fs::read_to_string(dir.join("expected.crv")).unwrap();
        let expected_report: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("expected.report.json")).unwrap())
                .unwrap();
        let result = html_to_carve(&html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(result.value, expected, "{}", dir.display());
        let expected_codes = expected_report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["code"].as_str().unwrap())
            .collect::<Vec<_>>();
        let actual_codes = result
            .report
            .diagnostics
            .iter()
            .map(|d| match d.code {
                HtmlImportDiagnosticCode::ElementDropped => "element-dropped",
                HtmlImportDiagnosticCode::ElementUnwrapped => "element-unwrapped",
                HtmlImportDiagnosticCode::AttributeDropped => "attribute-dropped",
                HtmlImportDiagnosticCode::StyleUnmapped => "style-unmapped",
                HtmlImportDiagnosticCode::TableDegraded => "table-degraded",
                HtmlImportDiagnosticCode::RawPreserved => "raw-preserved",
            })
            .collect::<Vec<_>>();
        assert_eq!(actual_codes, expected_codes, "{}", dir.display());
    }
}
