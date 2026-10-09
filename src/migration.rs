use crate::{
    bbcode_to_carve, djot_to_carve, html_to_carve, BbcodeImportError, HtmlImportAdapter,
    HtmlImportError, HtmlImportMode, HtmlImportOptions, HtmlImportSeverity,
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

fn assessed(
    source: &str,
    value: String,
    source_format: SourceFormat,
    has_known_losses: bool,
) -> MigrationResult {
    static LITERAL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = LITERAL.get_or_init(|| {
        regex::Regex::new(r"\A[\p{L}\p{N}]+(?: [\p{L}\p{N}]+)*\z").expect("literal text pattern")
    });
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let literal = normalized.trim_end_matches('\n');
    if !has_known_losses
        && (literal.is_empty() || pattern.is_match(literal))
        && value.trim_end_matches('\n') == literal
    {
        return MigrationResult {
            value,
            report: MigrationReport {
                schema_version: 2,
                source_format,
                mode: None,
                adapter: None,
                diagnostics: vec![MigrationDiagnostic {
                    code: "literal-text-verified".to_owned(),
                    message: "Verified the complete input as literal text.".to_owned(),
                    severity: HtmlImportSeverity::Info,
                    fidelity: MigrationFidelity::Preserved,
                    confidence: MigrationConfidence::Exact,
                    path: None,
                }],
            },
        };
    }
    let diagnostics = vec![MigrationDiagnostic {
        code: "fidelity-unverified".to_owned(),
        message: if source_format == SourceFormat::Markdown {
            "Markdown construct assessment is incomplete.".to_owned()
        } else {
            format!(
            "Fidelity was not reported by the {} importer; dropped is a conservative worst-case release-gate classification",
            source_format.as_str()
        )
        },
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

/// Migrate Markdown and return its fidelity report.
///
/// # Panics
///
/// Panics if the canonical writer cannot represent the imported document. Use
/// [`try_migrate_markdown`] to handle that error.
pub fn migrate_markdown(source: &str) -> MigrationResult {
    try_migrate_markdown(source).expect("the Markdown import cannot be written as Carve")
}

/// Migrate Markdown while preserving a typed canonical-writer failure.
pub fn try_migrate_markdown(source: &str) -> Result<MigrationResult, crate::RenderCarveError> {
    let (value, losses, notices) = crate::markdown_import::markdown_to_carve_with_losses(source)?;
    let assessment = crate::markdown_assessment::assess(source, &value);
    if assessment.complete
        && notices.is_empty()
        && losses
            .iter()
            .all(|loss| loss.message == crate::html_import::ORDERED_TASK_ITEM_UNSPELLABLE)
        && losses.len()
            <= assessment
                .diagnostics
                .iter()
                .filter(|row| row.code == "structure-unspellable")
                .count()
    {
        let mut literal = assessed(source, value.clone(), SourceFormat::Markdown, false);
        if literal
            .report
            .diagnostics
            .first()
            .is_some_and(|row| row.code == "literal-text-verified")
        {
            literal.report.diagnostics[0].path = Some("line:1".to_owned());
            return Ok(literal);
        }
        return Ok(MigrationResult {
            value,
            report: MigrationReport {
                schema_version: 2,
                source_format: SourceFormat::Markdown,
                mode: None,
                adapter: None,
                diagnostics: assessment.diagnostics,
            },
        });
    }
    let mut result = assessed(source, value, SourceFormat::Markdown, !losses.is_empty());
    if result
        .report
        .diagnostics
        .first()
        .is_some_and(|row| row.code == "literal-text-verified")
    {
        result.report.diagnostics[0].path = Some("line:1".to_owned());
    }
    result
        .report
        .diagnostics
        .extend(losses.into_iter().map(|loss| MigrationDiagnostic {
            code: "structure-unspellable".to_owned(),
            message: loss.message,
            severity: HtmlImportSeverity::Warning,
            fidelity: MigrationFidelity::Dropped,
            confidence: MigrationConfidence::Exact,
            path: loss.line.map(|line| format!("line:{line}")),
        }));
    // Reported, not a loss: the block's content survives byte-exact, and the
    // reader is told that a `---` run changed meaning (markup-carve/carve#2799).
    result
        .report
        .diagnostics
        .extend(notices.into_iter().map(|message| MigrationDiagnostic {
            code: "frontmatter-synthesized".to_owned(),
            message,
            severity: HtmlImportSeverity::Info,
            fidelity: MigrationFidelity::Preserved,
            confidence: MigrationConfidence::Exact,
            path: None,
        }));
    Ok(result)
}

pub fn migrate_djot(source: &str) -> MigrationResult {
    assessed(source, djot_to_carve(source), SourceFormat::Djot, false)
}

pub fn migrate_bbcode(source: &str) -> Result<MigrationResult, BbcodeImportError> {
    Ok(assessed(
        source,
        bbcode_to_carve(source)?,
        SourceFormat::Bbcode,
        false,
    ))
}

#[cfg(test)]
mod evidence_tests {
    use super::*;

    #[test]
    fn changed_output_or_known_loss_cannot_verify_literal_text() {
        for (value, known_loss) in [("changed", false), ("hello", true)] {
            let result = assessed(
                "hello",
                value.to_owned(),
                SourceFormat::Markdown,
                known_loss,
            );
            assert_eq!(result.report.diagnostics[0].code, "fidelity-unverified");
        }
    }
}
