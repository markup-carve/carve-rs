//! `thematic-break-spelling`: `carve fmt` writes every break `***` (carve#4973).

use super::{walk_all_blocks, LintWarning};
use crate::ast::{BlockNode, Document};

pub(super) fn collect(
    source: &str,
    doc: &Document,
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    let mut visit = |block: &BlockNode| {
        let BlockNode::ThematicBreak(rule) = block else {
            return;
        };
        // An included break's span is measured in another file.
        let Some(pos) = rule.pos.as_ref().filter(|pos| pos.file.is_none()) else {
            return;
        };
        let start = to_byte(pos.start_offset);
        let Some(raw) = source
            .get(start..to_byte(pos.end_offset).max(start))
            .map(str::trim_end)
        else {
            return;
        };
        if raw.is_empty() || raw == "***" {
            return;
        }
        out.push(LintWarning {
            file: None,
            included_by: Vec::new(),
            line: pos.start_line.max(1),
            column: pos.start_column.max(1),
            rule: "thematic-break-spelling",
            message: format!(
                "This thematic break is written \"{raw}\"; the canonical spelling is \"***\"."
            ),
            start,
            end: start + raw.len(),
        });
    };
    walk_all_blocks(&doc.children, &mut visit);
    for body in doc.footnote_defs.values() {
        walk_all_blocks(body, &mut visit);
    }
}
