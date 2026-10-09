//! A raw span the Markdown importer writes may end a content line in
//! whitespace, which CARVE-P2-025 drops (markup-carve/carve#2804).

use carve::{
    migrate_markdown, HtmlImportSeverity, MigrationConfidence, MigrationDiagnostic,
    MigrationFidelity,
};

fn rows(markdown: &str) -> Vec<MigrationDiagnostic> {
    migrate_markdown(markdown)
        .report
        .diagnostics
        .into_iter()
        .filter(|row| row.code == "raw-span-whitespace-trimmed")
        .collect()
}

fn gated(markdown: &str) -> Vec<String> {
    migrate_markdown(markdown)
        .report
        .diagnostics
        .into_iter()
        .filter(|row| {
            matches!(
                row.fidelity,
                MigrationFidelity::Degraded | MigrationFidelity::Dropped
            )
        })
        .map(|row| row.code)
        .collect()
}

#[test]
fn reports_the_loss_with_the_fields_the_ruling_names() {
    let rows = rows("intro\n\n<a href=\"foo  \nbar\">\n");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].code, "raw-span-whitespace-trimmed");
    assert_eq!(
        rows[0].message,
        "A raw span ends a content line in whitespace, which Carve drops; \
         the whitespace did not reach the converted source"
    );
    assert_eq!(rows[0].severity, HtmlImportSeverity::Warning);
    assert_eq!(rows[0].fidelity, MigrationFidelity::Degraded);
    assert_eq!(rows[0].confidence, MigrationConfidence::Exact);
    assert_eq!(rows[0].path.as_deref(), Some("line:3"));
}

#[test]
fn reports_a_tab_the_same_way() {
    let rows = rows("<a href=\"foo\t\nbar\">\n");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].fidelity, MigrationFidelity::Degraded);
}

#[test]
fn counts_the_line_in_the_input() {
    let rows = rows("[a]: /x\n\n<a href=\"foo  \nbar\">\n");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path.as_deref(), Some("line:3"));
}

#[test]
fn stays_silent_where_the_whitespace_is_not_at_a_line_end() {
    assert!(rows("x <a href=\"foo  bar\"> y\n").is_empty());
}

#[test]
fn stays_silent_where_the_raw_span_has_no_trailing_whitespace() {
    assert!(rows("x <a href=\"foo\">\n").is_empty());
}

#[test]
fn names_a_loss_the_engine_really_takes() {
    let value = migrate_markdown("<a href=\"foo  \nbar\">\n").value;
    assert!(value.contains("foo  \n"));
    let html = carve::render_html(&carve::parse(&value)).expect("render");
    assert!(html.contains("<a href=\"foo\nbar\">"), "{html}");
}

#[test]
fn reaches_the_default_loss_gate_where_a_clean_document_reaches_nothing() {
    assert!(gated("<a href=\"foo  \nbar\">\n")
        .iter()
        .any(|code| code == "raw-span-whitespace-trimmed"));
    assert!(gated("one two\n").is_empty());
}
