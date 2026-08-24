use crate::{parse_with_source_layout, BlockNode, InlineNode, Pos};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessibilitySeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessibilityDiagnostic {
    pub rule: &'static str,
    pub severity: AccessibilitySeverity,
    pub message: String,
    pub start_offset: Option<usize>,
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
