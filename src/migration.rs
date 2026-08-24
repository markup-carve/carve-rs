use crate::{
    djot_to_carve, html_to_carve, markdown_to_carve, HtmlImportDiagnosticCode, HtmlImportError,
    HtmlImportOptions, HtmlImportSeverity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Html,
    Markdown,
    Djot,
}

impl SourceFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Markdown => "markdown",
            Self::Djot => "djot",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationFidelity {
    Carried,
    Degraded,
    Dropped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationConfidence {
    Exact,
    Inferred,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: HtmlImportSeverity,
    pub fidelity: MigrationFidelity,
    pub confidence: MigrationConfidence,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub schema_version: u32,
    pub source_format: SourceFormat,
    pub diagnostics: Vec<MigrationDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationResult {
    pub value: String,
    pub report: MigrationReport,
}

fn fidelity(code: HtmlImportDiagnosticCode) -> MigrationFidelity {
    match code {
        HtmlImportDiagnosticCode::ElementDropped
        | HtmlImportDiagnosticCode::AttributeDropped
        | HtmlImportDiagnosticCode::StructureUnspellable => MigrationFidelity::Dropped,
        HtmlImportDiagnosticCode::StyleUnmapped
        | HtmlImportDiagnosticCode::TableDegraded
        | HtmlImportDiagnosticCode::EncodingAssumed
        | HtmlImportDiagnosticCode::DiagnosticsTruncated => MigrationFidelity::Degraded,
        HtmlImportDiagnosticCode::ElementUnwrapped
        | HtmlImportDiagnosticCode::RawPreserved
        | HtmlImportDiagnosticCode::StructureSplit => MigrationFidelity::Carried,
    }
}

pub fn migrate_html(
    source: &str,
    options: &HtmlImportOptions,
) -> Result<MigrationResult, HtmlImportError> {
    let result = html_to_carve(source, options)?;
    let diagnostics = result
        .report
        .diagnostics
        .into_iter()
        .map(|diagnostic| MigrationDiagnostic {
            code: diagnostic.code.as_str().to_owned(),
            message: diagnostic.message,
            severity: diagnostic.severity,
            fidelity: fidelity(diagnostic.code),
            confidence: if diagnostic.code == HtmlImportDiagnosticCode::EncodingAssumed {
                MigrationConfidence::Inferred
            } else {
                MigrationConfidence::Exact
            },
            path: diagnostic.path,
        })
        .collect();
    Ok(MigrationResult {
        value: result.value,
        report: MigrationReport {
            schema_version: 1,
            source_format: SourceFormat::Html,
            diagnostics,
        },
    })
}

fn exact(value: String, source_format: SourceFormat) -> MigrationResult {
    MigrationResult {
        value,
        report: MigrationReport {
            schema_version: 1,
            source_format,
            diagnostics: Vec::new(),
        },
    }
}

pub fn migrate_markdown(source: &str) -> MigrationResult {
    exact(markdown_to_carve(source), SourceFormat::Markdown)
}

pub fn migrate_djot(source: &str) -> MigrationResult {
    exact(djot_to_carve(source), SourceFormat::Djot)
}
