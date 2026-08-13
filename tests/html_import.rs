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
                HtmlImportDiagnosticCode::DiagnosticsTruncated => "diagnostics-truncated",
            })
            .collect::<Vec<_>>();
        assert_eq!(actual_codes, expected_codes, "{}", dir.display());
    }
}

/// PART 9 §4a, carve#1159. The renderer emits a quote's attribution as a
/// `<footer>` inside the `<blockquote>`, so an importer that read it as an
/// ordinary second paragraph made the engine's own HTML un-round-trippable.
#[test]
fn a_trailing_footer_in_a_quote_is_its_attribution() {
    let result = html_to_carve(
        "<blockquote><p>To be</p><footer>Hamlet</footer></blockquote>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.value, "> To be\n^ Hamlet\n");
}

/// A quote has ONE attribution, so a second footer cannot join it. The LAST is
/// the one this renderer emits and the one an author puts after the quoted
/// text; the earlier footer stays an ordinary block rather than being dropped.
#[test]
fn the_last_footer_is_the_attribution_and_the_others_stay() {
    let result = html_to_carve(
        "<blockquote><footer>First</footer><p>To be</p><footer>Hamlet</footer></blockquote>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.value, "> First\n>\n> To be\n^ Hamlet\n");
}

/// The slot holds INLINE content, so a footer carrying blocks does not fit it.
/// Flattening one would run its paragraphs together with no separator, so it
/// stays ordinary quoted content instead - every word survives, which is the
/// better answer when the shape cannot be represented. carve-js and carve-php
/// agree byte for byte.
#[test]
fn a_footer_carrying_blocks_stays_quoted_content() {
    let result = html_to_carve(
        "<blockquote><p>quote</p><footer><p>By <strong>A</strong></p><p>Work</p></footer></blockquote>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.value, "> quote\n>\n> By *A*\n>\n> Work\n");
}

/// PART 10 §T9 gives every `th` a `scope` from its POSITION, so importing that
/// value back would write this engine's own output in as if the author had
/// typed it. A value the default cannot explain is a different thing: `colgroup`
/// and `rowgroup` have no marker spelling and no positional derivation, so an
/// authored one is the only way to get them and dropping it is lossy
/// (carve-rs#944).
#[test]
fn an_authored_table_cell_scope_survives_the_import() {
    let result = html_to_carve(
        "<table><thead><tr><th scope=\"colgroup\">A</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert!(
        result.value.contains("scope=colgroup"),
        "the authored scope was dropped: {}",
        result.value
    );
}

#[test]
fn a_scope_that_only_restates_the_positional_default_is_dropped() {
    // The other half, and the one that must not regress: `col` on a cell in the
    // head-row run is exactly what the renderer emits from position, so keeping
    // it would round-trip the generator's own output back into the source.
    for html in [
        "<table><thead><tr><th scope=\"col\">A</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>",
        "<table><tbody><tr><th scope=\"row\">A</th><td>1</td></tr></tbody></table>",
    ] {
        let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
        assert!(
            !result.value.contains("scope"),
            "a positional scope was imported: {}",
            result.value
        );
    }
}
