use crate::{
    bbcode_to_carve, djot_to_carve, html_to_carve, markdown_to_carve, BbcodeImportError,
    HtmlImportDiagnosticCode, HtmlImportError, HtmlImportOptions, HtmlImportSeverity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Html,
    Markdown,
    Djot,
    Bbcode,
}

impl SourceFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Markdown => "markdown",
            Self::Djot => "djot",
            Self::Bbcode => "bbcode",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationFidelity {
    Preserved,
    Normalized,
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
        HtmlImportDiagnosticCode::ElementUnwrapped => MigrationFidelity::Normalized,
        // `AttributePreserved` is PRESERVED and not DROPPED: it is the row that
        // says an attribute reached the output inside preserved raw bytes, so
        // filing it under `Dropped` beside `AttributeDropped` would reintroduce
        // the same false claim one layer up (markup-carve/carve-js#1468).
        HtmlImportDiagnosticCode::AttributePreserved | HtmlImportDiagnosticCode::RawPreserved => {
            MigrationFidelity::Preserved
        }
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
            schema_version: 2,
            source_format: SourceFormat::Html,
            diagnostics,
        },
    })
}

fn normalized(source: &str, value: String, source_format: SourceFormat) -> MigrationResult {
    let diagnostics = if value == source {
        Vec::new()
    } else {
        vec![MigrationDiagnostic {
            code: "syntax-normalized".to_owned(),
            message: format!(
                "Converted {} syntax to canonical Carve source",
                source_format.as_str()
            ),
            severity: HtmlImportSeverity::Info,
            fidelity: MigrationFidelity::Normalized,
            confidence: MigrationConfidence::Exact,
            path: None,
        }]
    };
    MigrationResult {
        value,
        report: MigrationReport {
            schema_version: 2,
            source_format,
            diagnostics,
        },
    }
}

pub fn migrate_markdown(source: &str) -> MigrationResult {
    normalized(source, markdown_to_carve(source), SourceFormat::Markdown)
}

pub fn migrate_djot(source: &str) -> MigrationResult {
    normalized(source, djot_to_carve(source), SourceFormat::Djot)
}

pub fn migrate_bbcode(source: &str) -> Result<MigrationResult, BbcodeImportError> {
    Ok(normalized(
        source,
        bbcode_to_carve(source)?,
        SourceFormat::Bbcode,
    ))
}
