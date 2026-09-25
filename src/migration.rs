use crate::{
    bbcode_to_carve, djot_to_carve, html_to_carve, markdown_to_carve, BbcodeImportError,
    HtmlImportAdapter, HtmlImportError, HtmlImportMode, HtmlImportOptions, HtmlImportSeverity,
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

/// The report's vocabulary is the IMPORTER's, not a second copy of it: the
/// classification belongs to the diagnostic code, and a migration report only
/// repeats what the importer already answered.
pub type MigrationFidelity = crate::html_import::ImportFidelity;
pub type MigrationConfidence = crate::html_import::ImportConfidence;

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
            fidelity: diagnostic.fidelity,
            confidence: diagnostic.confidence,
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

/// Migrate Markdown, or the writer's refusal.
///
/// The fallible form is the one to reach for on untrusted input: the Markdown
/// importer is the only one whose tree has no nesting bound of its own, so it is
/// the only one that can reach PART 9 §25's ceiling (carve-rs#1877).
pub fn try_migrate_markdown(source: &str) -> Result<MigrationResult, crate::RenderCarveError> {
    Ok(unverified(
        crate::try_markdown_to_carve(source)?,
        SourceFormat::Markdown,
    ))
}

/// Migrate Markdown, with an empty result where the writer refuses.
///
/// Prefer [`try_migrate_markdown`] for input you did not write.
pub fn migrate_markdown(source: &str) -> MigrationResult {
    unverified(markdown_to_carve(source), SourceFormat::Markdown)
}

pub fn migrate_djot(source: &str) -> MigrationResult {
    unverified(djot_to_carve(source), SourceFormat::Djot)
}

pub fn migrate_bbcode(source: &str) -> Result<MigrationResult, BbcodeImportError> {
    Ok(unverified(bbcode_to_carve(source)?, SourceFormat::Bbcode))
}
