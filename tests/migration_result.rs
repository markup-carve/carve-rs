use carve::{
    migrate_djot, migrate_html, migrate_markdown, HtmlImportOptions, MigrationFidelity,
    SourceFormat,
};

#[test]
fn every_source_format_returns_the_same_result_shape() {
    let markdown = migrate_markdown("**strong**");
    assert_eq!(markdown.value, "*strong*\n");
    assert_eq!(markdown.report.schema_version, 1);
    assert_eq!(markdown.report.source_format, SourceFormat::Markdown);
    assert!(markdown.report.diagnostics.is_empty());

    let djot = migrate_djot("_emphasis_");
    assert_eq!(djot.value, "/emphasis/");
    assert_eq!(djot.report.source_format, SourceFormat::Djot);
    assert!(djot.report.diagnostics.is_empty());

    let html = migrate_html("<p><blink>text</blink></p>", &HtmlImportOptions::default())
        .expect("HTML import succeeds");
    assert_eq!(html.report.source_format, SourceFormat::Html);
    assert!(!html.report.diagnostics.is_empty());
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
