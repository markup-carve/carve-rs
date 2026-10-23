//! Migration results and diagnostics for supported source formats.

use crate::markdown_import::MarkdownLossKind;
use crate::{
    html_to_carve, BbcodeImportError, HtmlImportAdapter, HtmlImportError, HtmlImportMode,
    HtmlImportOptions, HtmlImportSeverity,
};

/// Source language being migrated to Carve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    /// HTML input.
    Html,
    /// Markdown input.
    Markdown,
    /// Djot input.
    Djot,
    /// BBCode input.
    Bbcode,
}

impl SourceFormat {
    /// Return the lowercase source-format name.
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
/// Confidence vocabulary shared with HTML import reports.
pub type MigrationConfidence = crate::html_import::ImportConfidence;

/// One finding produced during migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationDiagnostic {
    /// Stable finding code.
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
    /// Severity for presentation.
    pub severity: HtmlImportSeverity,
    /// Extent to which the source content survived.
    pub fidelity: MigrationFidelity,
    /// Confidence in the migration decision.
    pub confidence: MigrationConfidence,
    /// Path to the source construct, when available.
    pub path: Option<String>,
}

/// Metadata and findings for one migration operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    /// Version of the migration report schema.
    pub schema_version: u32,
    /// Language of the input source.
    pub source_format: SourceFormat,
    /// HTML import mode, when the input was HTML.
    pub mode: Option<HtmlImportMode>,
    /// HTML source adapter, when the input was HTML.
    pub adapter: Option<HtmlImportAdapter>,
    /// Findings in the order reported by the importer.
    pub diagnostics: Vec<MigrationDiagnostic>,
}

/// Migrated Carve source and its report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationResult {
    /// Resulting Carve source.
    pub value: String,
    /// Decisions and losses recorded during migration.
    pub report: MigrationReport,
}

/// Migrate HTML to Carve source with importer options and a report.
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
    try_migrate_markdown_with_fragments(source, true)
}

/// Directory migration checks links after all target files have been converted.
pub fn try_migrate_markdown_for_directory(
    source: &str,
) -> Result<MigrationResult, crate::RenderCarveError> {
    try_migrate_markdown_with_fragments(source, false)
}

/// The directory migration, plus each changed destination's authored spelling
/// for the link pass (CARVE-P2-031).
#[doc(hidden)]
pub fn try_migrate_markdown_for_directory_with_destinations(
    source: &str,
) -> Result<(MigrationResult, std::collections::HashMap<String, String>), crate::RenderCarveError> {
    let (converted, authored) = crate::markdown_import::with_authored_destinations(|| {
        crate::markdown_import::markdown_to_carve_with_losses_mode(source, false)
    });
    let (value, losses, notices) = converted?;
    Ok((
        assess_markdown_migration(source, value, losses, notices)?,
        authored,
    ))
}

fn try_migrate_markdown_with_fragments(
    source: &str,
    rewrite_heading_fragments: bool,
) -> Result<MigrationResult, crate::RenderCarveError> {
    let (converted, authored) = crate::markdown_import::with_authored_destinations(|| {
        crate::markdown_import::markdown_to_carve_with_losses_mode(source, false)
    });
    let (value, losses, notices) = converted?;
    if !rewrite_heading_fragments {
        return assess_markdown_migration(source, value, losses, notices);
    }
    let rewrite =
        crate::markdown_import::rewrite_same_file_heading_links_reported(value, &authored)?;
    let mut result = assess_markdown_migration(source, rewrite.unfixed, losses, notices)?;
    result.value = rewrite.value;
    result.report.diagnostics.extend(
        rewrite
            .fixups
            .into_iter()
            .map(|fixup| fragment_fixup_diagnostic(&fixup.destination, &fixup.from, &fixup.to)),
    );
    Ok(result)
}

/// The `link-fragment-case-rewritten` row for one rewritten link (CARVE-P2-031).
#[doc(hidden)]
pub fn fragment_fixup_diagnostic(destination: &str, from: &str, to: &str) -> MigrationDiagnostic {
    MigrationDiagnostic {
        code: "link-fragment-case-rewritten".to_owned(),
        message: format!(
            "Rewrote link fragment #{from} to #{to}, the one heading id it matches without regard to case."
        ),
        severity: HtmlImportSeverity::Info,
        fidelity: MigrationFidelity::Normalized,
        confidence: MigrationConfidence::Inferred,
        path: Some(format!("link:{destination}")),
    }
}

fn assess_markdown_migration(
    source: &str,
    value: String,
    losses: Vec<crate::markdown_import::MarkdownImportLoss>,
    notices: Vec<crate::markdown_import::FrontmatterNotice>,
) -> Result<MigrationResult, crate::RenderCarveError> {
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
    let extra_cell_lines = assessment.extra_cell_lines;
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
        .extend(losses.into_iter().map(|loss| {
            MigrationDiagnostic {
                code: match loss.kind {
                    MarkdownLossKind::Unspellable => "structure-unspellable",
                    MarkdownLossKind::RawSpanWhitespaceTrimmed => "raw-span-whitespace-trimmed",
                    MarkdownLossKind::CarrierMarkersDamaged => "carrier-markers-damaged",
                    MarkdownLossKind::CarrierMarkerDropped => "carrier-marker-dropped",
                    MarkdownLossKind::RawCodeFallback => "raw-code-fallback",
                }
                .to_owned(),
                message: loss.message,
                severity: HtmlImportSeverity::Warning,
                // Degraded where the span and its text survive and a whitespace
                // run inside it does not (markup-carve/carve#2804).
                fidelity: match loss.kind {
                    MarkdownLossKind::Unspellable => MigrationFidelity::Dropped,
                    MarkdownLossKind::RawSpanWhitespaceTrimmed
                    | MarkdownLossKind::CarrierMarkersDamaged
                    | MarkdownLossKind::CarrierMarkerDropped
                    | MarkdownLossKind::RawCodeFallback => MigrationFidelity::Degraded,
                },
                // A damaged set is a FALLBACK reading: the markers came through
                // as the raw HTML they are and no container was guessed.
                confidence: match loss.kind {
                    MarkdownLossKind::CarrierMarkersDamaged
                    | MarkdownLossKind::CarrierMarkerDropped => MigrationConfidence::Fallback,
                    _ => MigrationConfidence::Exact,
                },
                path: loss.line.map(|line| format!("line:{line}")),
            }
        }));
    result
        .report
        .diagnostics
        .extend(extra_cell_lines.into_iter().map(|line| {
            MigrationDiagnostic {
                code: "markdown-table-extra-cells".to_owned(),
                message:
                    "GFM ignores body cells beyond the header width; their content was omitted"
                        .to_owned(),
                severity: HtmlImportSeverity::Warning,
                fidelity: MigrationFidelity::Dropped,
                confidence: MigrationConfidence::Exact,
                path: Some(format!("line:{line}")),
            }
        }));
    // Reported, not a loss: the block's content survives byte-exact, and the
    // reader is told that a `---` run changed meaning (markup-carve/carve#2799).
    //
    // `normalized` because an alternate block form was resolved - the same
    // bytes could have read as a thematic break over a setext heading - and
    // `line:1` because a synthesized block starts there. The confidence
    // follows the opener (markup-carve/carve#2806).
    result
        .report
        .diagnostics
        .extend(notices.into_iter().map(|notice| MigrationDiagnostic {
            code: "frontmatter-synthesized".to_owned(),
            message: notice.message,
            severity: HtmlImportSeverity::Info,
            fidelity: MigrationFidelity::Normalized,
            confidence: if notice.typed_opener {
                MigrationConfidence::Exact
            } else {
                MigrationConfidence::Inferred
            },
            path: Some("line:1".to_owned()),
        }));
    Ok(result)
}

/// Migrate Djot to Carve source and report conversion decisions.
pub fn migrate_djot(source: &str) -> MigrationResult {
    let stripped = crate::djot_migrate::strip_footnote_definition_attributes(source);
    let (value, losses) = crate::djot_migrate::djot_to_carve_with_losses(source);
    let mut result = assessed(
        source,
        value,
        SourceFormat::Djot,
        !stripped.losses.is_empty() || !losses.is_empty(),
    );
    result.report.diagnostics.extend(losses);
    for line in stripped.losses {
        result.report.diagnostics.push(MigrationDiagnostic {
            code: "djot-footnote-definition-attributes-dropped".into(),
            message: "Carve cannot represent attributes on a footnote definition; they were dropped instead of applying them to later content.".into(),
            severity: HtmlImportSeverity::Warning,
            fidelity: MigrationFidelity::Dropped,
            confidence: MigrationConfidence::Exact,
            path: Some(format!("line:{line}")),
        });
    }
    result
}

/// Migrate BBCode to Carve source, or return an input-boundary error.
pub fn migrate_bbcode(source: &str) -> Result<MigrationResult, BbcodeImportError> {
    let (value, dropped) = crate::bbcode_migrate::bbcode_to_carve_with_drops(source)?;
    let mut result = assessed(source, value, SourceFormat::Bbcode, false);
    result.report.diagnostics.extend(
        dropped
            .into_iter()
            .map(|drop| MigrationDiagnostic {
                code: "bbcode-quote-field-dropped".to_owned(),
                message: if drop.unnamed {
                    format!("Dropped the unnamed quote attribution text \"{}\", which an explicit name= replaced.", drop.field)
                } else {
                    format!("Dropped the quote attribution field {}.", drop.field)
                },
                severity: HtmlImportSeverity::Warning,
                fidelity: MigrationFidelity::Dropped,
                confidence: MigrationConfidence::Exact,
                path: Some(format!("line:{}", drop.line)),
            }),
    );
    Ok(result)
}

#[cfg(test)]
mod evidence_tests {
    use super::*;

    #[test]
    fn same_file_fragment_rewrite_is_a_fixup_beside_a_complete_assessment() {
        let result = migrate_markdown("# Install Steps\n\nSee [steps](#install-steps).\n");
        assert_eq!(
            result.value,
            "# Install Steps\n\nSee [steps](#Install-Steps).\n"
        );
        let codes: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .map(|row| row.code.as_str())
            .collect();
        assert_eq!(
            codes,
            [
                "markdown-atx-heading",
                "markdown-paragraph",
                "markdown-link",
                "link-fragment-case-rewritten"
            ]
        );
        let row = result.report.diagnostics.last().unwrap();
        assert_eq!(row.message, "Rewrote link fragment #install-steps to #Install-Steps, the one heading id it matches without regard to case.");
        assert_eq!(row.severity, HtmlImportSeverity::Info);
        assert_eq!(row.fidelity, MigrationFidelity::Normalized);
        assert_eq!(row.confidence, MigrationConfidence::Inferred);
        assert_eq!(row.path.as_deref(), Some("link:#install-steps"));
        for markdown in [
            "# Twin\n\n# twin\n\n[x](#TWIN)\n",
            "# Install Steps\n\n[x](#Install-Steps)\n",
        ] {
            assert!(!migrate_markdown(markdown)
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "link-fragment-case-rewritten"));
        }
        let unicode = migrate_markdown("# ΟΔΟΣ\n\n[street](#οδος)\n");
        assert_eq!(
            unicode.report.diagnostics.last().unwrap().message,
            "Rewrote link fragment #οδος to #ΟΔΟΣ, the one heading id it matches without regard to case."
        );
    }

    /// CARVE-P2-031 (carve#3356): the row names the link as the author wrote it.
    #[test]
    fn a_fragment_fixup_names_the_authored_spelling() {
        let result = migrate_markdown("# Über Uns\n\nSee [a](#über-uns).\n");
        assert_eq!(result.value, "# Über Uns\n\nSee [a](#Über-Uns).\n");
        let row = result.report.diagnostics.last().unwrap();
        assert_eq!(row.code, "link-fragment-case-rewritten");
        assert_eq!(row.severity, HtmlImportSeverity::Info);
        assert_eq!(row.fidelity, MigrationFidelity::Normalized);
        assert_eq!(row.confidence, MigrationConfidence::Inferred);
        assert_eq!(row.path.as_deref(), Some("link:#über-uns"));
        assert_eq!(row.message, "Rewrote link fragment #über-uns to #Über-Uns, the one heading id it matches without regard to case.");

        // Distinct authored spellings retain distinct destination keys.
        let twins = migrate_markdown("# Über Uns\n\nSee [a](#über-uns) and [b](#%C3%BCber-uns).\n");
        let paths: Vec<_> = twins
            .report
            .diagnostics
            .iter()
            .filter(|row| row.code == "link-fragment-case-rewritten")
            .map(|row| row.path.as_deref().unwrap())
            .collect();
        assert_eq!(paths, ["link:#über-uns", "link:#%C3%BCber-uns"]);
    }

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
