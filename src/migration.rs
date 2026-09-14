use crate::{
    bbcode_to_carve, djot_to_carve, html_to_carve, markdown_to_carve, BbcodeImportError,
    HtmlImportAdapter, HtmlImportDiagnosticCode, HtmlImportError, HtmlImportMode,
    HtmlImportOptions, HtmlImportSeverity,
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

impl MigrationFidelity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preserved => "preserved",
            Self::Normalized => "normalized",
            Self::Degraded => "degraded",
            Self::Dropped => "dropped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationConfidence {
    Exact,
    Inferred,
    Fallback,
}

impl MigrationConfidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Inferred => "inferred",
            Self::Fallback => "fallback",
        }
    }
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
    pub mode: Option<HtmlImportMode>,
    pub adapter: Option<HtmlImportAdapter>,
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
        | HtmlImportDiagnosticCode::StructureUnspellable
        | HtmlImportDiagnosticCode::DiagnosticsTruncated => MigrationFidelity::Dropped,
        HtmlImportDiagnosticCode::ElementUnwrapped
        | HtmlImportDiagnosticCode::StyleUnmapped
        | HtmlImportDiagnosticCode::TableDegraded
        | HtmlImportDiagnosticCode::EncodingAssumed
        | HtmlImportDiagnosticCode::RawPreserved => MigrationFidelity::Degraded,
        // `AttributePreserved` is PRESERVED and not DROPPED: it is the row that
        // says an attribute reached the output inside preserved raw bytes, so
        // filing it under `Dropped` beside `AttributeDropped` would reintroduce
        // the same false claim one layer up (markup-carve/carve-js#1468).
        HtmlImportDiagnosticCode::AttributePreserved => MigrationFidelity::Preserved,
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
            confidence: match diagnostic.code {
                HtmlImportDiagnosticCode::EncodingAssumed => MigrationConfidence::Inferred,
                HtmlImportDiagnosticCode::DiagnosticsTruncated => MigrationConfidence::Fallback,
                _ => MigrationConfidence::Exact,
            },
            path: diagnostic.path,
        })
        .collect();
    Ok(MigrationResult {
        value: result.value,
        report: MigrationReport {
            schema_version: 2,
            source_format: SourceFormat::Html,
            mode: Some(result.report.mode),
            adapter: Some(result.report.adapter),
            diagnostics,
        },
    })
}

fn unverified(value: String, source_format: SourceFormat) -> MigrationResult {
    let diagnostics = vec![MigrationDiagnostic {
        code: "fidelity-unverified".to_owned(),
        message: format!(
            "Fidelity was not reported by the {} importer; dropped is a conservative worst-case release-gate classification",
            source_format.as_str()
        ),
        severity: HtmlImportSeverity::Warning,
        fidelity: MigrationFidelity::Dropped,
        confidence: MigrationConfidence::Fallback,
        path: None,
    }];
    MigrationResult {
        value,
        report: MigrationReport {
            schema_version: 2,
            source_format,
            mode: None,
            adapter: None,
            diagnostics,
        },
    }
}

pub fn migrate_markdown(source: &str) -> MigrationResult {
    unverified(markdown_to_carve(source), SourceFormat::Markdown)
}

pub fn migrate_djot(source: &str) -> MigrationResult {
    unverified(djot_to_carve(source), SourceFormat::Djot)
}

pub fn migrate_bbcode(source: &str) -> Result<MigrationResult, BbcodeImportError> {
    Ok(unverified(bbcode_to_carve(source)?, SourceFormat::Bbcode))
}
