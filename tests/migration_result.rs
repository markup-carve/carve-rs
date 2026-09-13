use carve::{
    migrate_bbcode, migrate_djot, migrate_html, migrate_markdown, HtmlImportMode,
    HtmlImportOptions, HtmlImportSeverity, MigrationConfidence, MigrationFidelity, SourceFormat,
};

#[test]
fn every_source_format_returns_the_same_result_shape() {
    let markdown = migrate_markdown("**strong**");
    assert_eq!(markdown.value, "*strong*\n");
    assert_eq!(markdown.report.schema_version, 2);
    assert_eq!(markdown.report.source_format, SourceFormat::Markdown);
    assert_eq!(markdown.report.diagnostics.len(), 1);
    assert_eq!(
        markdown.report.diagnostics[0].fidelity,
        MigrationFidelity::Degraded
    );
    assert_eq!(
        markdown.report.diagnostics[0].confidence,
        MigrationConfidence::Fallback
    );
    assert_eq!(
        markdown.report.diagnostics[0].severity,
        HtmlImportSeverity::Warning
    );

    let djot = migrate_djot("_emphasis_");
    assert_eq!(djot.value, "/emphasis/");
    assert_eq!(djot.report.source_format, SourceFormat::Djot);
    assert_eq!(djot.report.diagnostics[0].code, "fidelity-unverified");

    let bbcode = migrate_bbcode("[b]strong[/b]").expect("BBCode import succeeds");
    assert_eq!(bbcode.report.schema_version, 2);
    assert_eq!(bbcode.report.source_format, SourceFormat::Bbcode);
    assert_eq!(
        bbcode.report.diagnostics[0].fidelity,
        MigrationFidelity::Degraded
    );

    let html = migrate_html("<p><blink>text</blink></p>", &HtmlImportOptions::default())
        .expect("HTML import succeeds");
    assert_eq!(html.report.source_format, SourceFormat::Html);
    assert_eq!(html.report.mode, Some(HtmlImportMode::Safe));
    assert!(!html.report.diagnostics.is_empty());
}

#[test]
fn html_report_pins_degraded_preserved_and_truncated_classifications() {
    let unwrapped = migrate_html("<ruby>x<rt>y</rt></ruby>", &HtmlImportOptions::default())
        .expect("HTML import succeeds");
    assert!(unwrapped.report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "element-unwrapped"
            && diagnostic.fidelity == MigrationFidelity::Degraded
            && diagnostic.confidence == MigrationConfidence::Exact
    }));

    let preserved = migrate_html(
        "<unknown data-x=\"1\">x</unknown>",
        &HtmlImportOptions {
            mode: HtmlImportMode::Roundtrip,
            ..Default::default()
        },
    )
    .expect("roundtrip HTML import succeeds");
    assert!(preserved.report.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.code.as_str(),
            "raw-preserved" | "attribute-preserved"
        ) && diagnostic.fidelity == MigrationFidelity::Preserved
    }));

    let truncated = migrate_html(
        "<script>x</script><script>y</script>",
        &HtmlImportOptions {
            max_diagnostics: 1,
            ..Default::default()
        },
    )
    .expect("bounded HTML import succeeds");
    assert!(truncated.report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "diagnostics-truncated"
            && diagnostic.confidence == MigrationConfidence::Fallback
    }));
}

#[test]
fn html_loss_is_classified_for_callers() {
    let result = migrate_html(
        "<p><kbd kbd=lit>text</kbd></p>",
        &HtmlImportOptions::default(),
    )
    .expect("HTML import succeeds");
    let diagnostic = result
        .report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "attribute-dropped")
        .expect("the colliding semantic attribute cannot be represented");
    assert_eq!(diagnostic.fidelity, MigrationFidelity::Dropped);
}
