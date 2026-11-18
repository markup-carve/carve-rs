//! Entries of the diagnostics record (C2-A-173, C2-T-020).

use crate::diagnostic::Diagnostic;
use serde::Serialize;

/// A range in UTF-8 bytes of the source (C2-T-020).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    pub start_offset: usize,
    pub end_offset: usize,
}

/// One entry of the record, with the library fields of C2-T-020. The command
/// line adds `line` and `column`; the library does not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub code: String,
    /// `hint`, `warning` or `error`.
    pub severity: String,
    /// `parse`, `render-loss` or `lint`.
    pub channel: String,
    pub message: String,
    /// An RFC 6901 pointer into the tree, where the entry is about a node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
    /// The id of the included file the range is counted in (C2-N-063).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The HTML node name of an importer entry (C2-M-081).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreign_type: Option<String>,
}

impl From<&Diagnostic> for Entry {
    fn from(d: &Diagnostic) -> Self {
        Self {
            code: d.code.to_string(),
            severity: d.severity.to_string(),
            channel: d.channel.to_string(),
            message: d.message.clone(),
            path: d.path.clone(),
            range: d.pos.map(|p| Range {
                start_offset: p.start(),
                end_offset: p.end(),
            }),
            file: d.file.clone(),
            foreign_type: None,
        }
    }
}

/// C2-T-024: keeps the first `include_diagnostics` entries whose code is an
/// include cause and returns the record with the number held back.
pub(crate) fn cap_includes(diagnostics: &[Diagnostic]) -> (Vec<Entry>, usize) {
    let mut kept = 0usize;
    let mut suppressed = 0usize;
    let mut out = Vec::with_capacity(diagnostics.len());
    for d in diagnostics {
        if crate::tables::INCLUDE_CAUSES
            .iter()
            .any(|(_, code)| *code == d.code)
        {
            if kept == crate::tables::INCLUDE_DIAGNOSTICS {
                suppressed += 1;
                continue;
            }
            kept += 1;
        }
        out.push(Entry::from(d));
    }
    (out, suppressed)
}
