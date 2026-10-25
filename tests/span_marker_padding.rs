use carve::{
    html_to_carve, to_carve, to_html, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions,
};

#[test]
fn lone_span_glyphs_ignore_cell_padding() {
    for marker in ["<", "^"] {
        let variants = [
            format!("|a|{marker}|\n"),
            format!("|a| {marker}|\n"),
            format!("|a|{marker} |\n"),
            format!("|a| {marker} |\n"),
        ];
        let expected = to_html(&variants[0]);
        for source in variants {
            assert_eq!(to_html(&source), expected, "{source:?}");
        }
    }
}

#[test]
fn writer_escapes_literal_span_glyphs() {
    for glyph in ["<", "^"] {
        let source = format!("|a| \\{glyph} |\n");
        let written = to_carve(&source);
        assert_eq!(to_html(&written), to_html(&source));
        assert!(written.contains(&format!("\\{glyph}")), "{written:?}");
    }
    let header = to_carve("|a|= <|\n");
    assert!(header.contains("|= < |"), "{header:?}");
}

#[test]
fn imported_empty_left_body_cell_does_not_become_a_span() {
    let html = "<table><tr><td>a</td><td style=\"text-align:left\"></td></tr></table>";
    let result = html_to_carve(
        html,
        &HtmlImportOptions {
            mode: HtmlImportMode::Semantic,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!result.value.contains("|< "), "{:?}", result.value);
    assert_eq!(to_html(&result.value).matches("<td").count(), 2);
}

#[test]
fn empty_header_left_default_survives_a_delimiter_row() {
    let source = "|=< |\n|:--|\n| a |\n";
    let written = to_carve(source);
    assert_eq!(to_html(&written), to_html(source), "{written:?}");
    assert!(written.contains("|:--|"), "{written:?}");
}

#[test]
fn empty_header_top_default_survives_a_delimiter_row() {
    let source = "|=?^ |\n|:--|\n| a |\n";
    let written = to_carve(source);
    assert_eq!(to_html(&written), to_html(source), "{written:?}");
    assert!(written.contains("?^ "), "{written:?}");
}

#[test]
fn empty_header_default_without_delimiter_is_reported() {
    let error =
        carve::try_to_carve_with_options("|=< |\n| a |\n", &carve::Options::default()).unwrap_err();
    assert!(error.to_string().contains("empty header alignment default"));
}

#[test]
fn imported_empty_header_preserves_its_column_alignment_default() {
    let html = "<table><tr><th style=\"text-align:left\"></th></tr><tr><td>a</td></tr></table>";
    let result = html_to_carve(
        html,
        &HtmlImportOptions {
            mode: HtmlImportMode::Semantic,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!result
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == HtmlImportDiagnosticCode::StyleUnmapped));
    assert!(result.value.starts_with("{aligns=left}\n"));
    assert!(!result.value.contains("< "), "{:?}", result.value);
}

#[test]
fn imported_literal_span_glyph_stays_text() {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Semantic,
        ..Default::default()
    };
    for (html, glyph) in [
        ("<table><tr><td>&lt;</td></tr></table>", "<"),
        ("<table><tr><td>^</td></tr></table>", "^"),
    ] {
        let source = html_to_carve(html, &options).unwrap().value;
        assert!(source.contains(&format!("\\{glyph}")), "{source:?}");
        assert_eq!(to_html(&source).matches("<td").count(), 1);
    }
}
