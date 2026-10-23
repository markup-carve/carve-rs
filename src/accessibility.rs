//! Accessibility checks for Carve source.

use crate::{parse_with_source_layout, BlockNode, InlineNode, Pos};

/// Severity of an accessibility finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessibilitySeverity {
    /// The source may be hard to use but can still be rendered.
    Warning,
    /// The source violates a checked accessibility rule.
    Error,
}

/// An accessibility finding with an optional source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessibilityDiagnostic {
    /// Stable rule identifier.
    pub rule: &'static str,
    /// Finding severity.
    pub severity: AccessibilitySeverity,
    /// Explanation for the author.
    pub message: String,
    /// Start offset in source codepoints, when available.
    pub start_offset: Option<usize>,
    /// Exclusive end offset in source codepoints, when available.
    pub end_offset: Option<usize>,
}

fn range(pos: Option<&Pos>) -> (Option<usize>, Option<usize>) {
    (
        pos.map(|value| value.start_offset),
        pos.map(|value| value.end_offset),
    )
}

fn check_image(alt: &str, pos: Option<&Pos>, diagnostics: &mut Vec<AccessibilityDiagnostic>) {
    if !alt.is_empty() {
        return;
    }
    let (start_offset, end_offset) = range(pos);
    diagnostics.push(AccessibilityDiagnostic {
        rule: "a11y/image-alt",
        severity: AccessibilitySeverity::Error,
        message: "image has empty alternative text and is not marked decorative".into(),
        start_offset,
        end_offset,
    });
}

fn check_inlines(nodes: &[InlineNode], diagnostics: &mut Vec<AccessibilityDiagnostic>) {
    for node in nodes {
        if let InlineNode::Image(image) = node {
            check_image(&image.alt, image.pos.as_ref(), diagnostics);
        }
    }
}

/// Parse Carve source and report image and heading accessibility findings.
pub fn lint_accessibility(source: &str) -> Vec<AccessibilityDiagnostic> {
    let (document, _) = parse_with_source_layout(source);
    let mut diagnostics = Vec::new();
    let mut previous_heading = None;
    for block in &document.children {
        match block {
            BlockNode::Heading(heading) => {
                if previous_heading.is_some_and(|previous| heading.level > previous + 1) {
                    let (start_offset, end_offset) = range(heading.pos.as_ref());
                    diagnostics.push(AccessibilityDiagnostic {
                        rule: "a11y/heading-jump",
                        severity: AccessibilitySeverity::Warning,
                        message: format!(
                            "heading level jumps from {} to {}",
                            previous_heading.expect("checked above"),
                            heading.level
                        ),
                        start_offset,
                        end_offset,
                    });
                }
                previous_heading = Some(heading.level);
                check_inlines(&heading.children, &mut diagnostics);
            }
            BlockNode::Paragraph(paragraph) => {
                check_inlines(&paragraph.children, &mut diagnostics);
            }
            BlockNode::BlockImage(image) => {
                check_image(&image.alt, image.pos.as_ref(), &mut diagnostics);
            }
            _ => {}
        }
    }
    diagnostics
}
