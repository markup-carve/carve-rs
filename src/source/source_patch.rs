//! Stale-safe UTF-8 source patches that preserve every unmentioned byte.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceEditKind {
    Formatting,
    SyntaxMigration,
    QuickFix,
    Refactor,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
    pub kind: SourceEditKind,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceSuggestion {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
    pub kind: SourceEditKind,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePatch {
    pub version: u8,
    pub source_fingerprint: String,
    pub source_bytes: usize,
    #[serde(default)]
    pub edits: Vec<SourceEdit>,
    #[serde(default)]
    pub unresolved: Vec<SourceSuggestion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePatchError(String);

impl fmt::Display for SourcePatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SourcePatchError {}

pub fn source_fingerprint(source: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in source.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

/// Build the smallest single replacement that produces `replacement`.
pub fn create_source_patch(
    source: &str,
    replacement: &str,
    kind: SourceEditKind,
    code: impl Into<String>,
) -> SourcePatch {
    let before = source.as_bytes();
    let after = replacement.as_bytes();
    let mut start = before
        .iter()
        .zip(after)
        .take_while(|(left, right)| left == right)
        .count();
    while start > 0 && ((!source.is_char_boundary(start)) || (!replacement.is_char_boundary(start)))
    {
        start -= 1;
    }
    let mut old_end = before.len();
    let mut new_end = after.len();
    while old_end > start && new_end > start && before[old_end - 1] == after[new_end - 1] {
        old_end -= 1;
        new_end -= 1;
    }
    while !source.is_char_boundary(old_end) || !replacement.is_char_boundary(new_end) {
        old_end += 1;
        new_end += 1;
    }
    let edits = if source == replacement {
        Vec::new()
    } else {
        vec![SourceEdit {
            start,
            end: old_end,
            replacement: replacement[start..new_end].into(),
            kind,
            code: code.into(),
        }]
    };
    SourcePatch {
        version: 1,
        source_fingerprint: source_fingerprint(source),
        source_bytes: source.len(),
        edits,
        unresolved: Vec::new(),
    }
}

pub fn apply_source_patch(source: &str, patch: &SourcePatch) -> Result<String, SourcePatchError> {
    if patch.version != 1 {
        return Err(SourcePatchError("unsupported source patch version".into()));
    }
    if patch.source_bytes != source.len() || patch.source_fingerprint != source_fingerprint(source)
    {
        return Err(SourcePatchError(
            "source patch precondition does not match the source".into(),
        ));
    }
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    for edit in &patch.edits {
        if edit.start < cursor
            || edit.end < edit.start
            || edit.end > source.len()
            || !source.is_char_boundary(edit.start)
            || !source.is_char_boundary(edit.end)
            || edit.code.is_empty()
        {
            return Err(SourcePatchError(
                "source patch edits must be sorted, non-overlapping UTF-8 byte ranges".into(),
            ));
        }
        output.push_str(&source[cursor..edit.start]);
        output.push_str(&edit.replacement);
        cursor = edit.end;
    }
    output.push_str(&source[cursor..]);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patches_use_utf8_ranges_and_reject_stale_sources() {
        assert_eq!(source_fingerprint("lead ä\n"), "fnv1a64:c6f20701944350f0");
        let source = "lead ä\nbody   \ntail\n";
        let expected = "lead ä\nbody\ntail\n";
        let patch = create_source_patch(
            source,
            expected,
            SourceEditKind::Formatting,
            "canonical-format",
        );
        assert_eq!(patch.edits[0].start, 12);
        assert_eq!(apply_source_patch(source, &patch).unwrap(), expected);
        assert!(apply_source_patch("changed", &patch).is_err());
        let wire = serde_json::to_string(&patch).unwrap();
        assert!(wire.contains("\"sourceFingerprint\":\"fnv1a64:"));
        assert_eq!(serde_json::from_str::<SourcePatch>(&wire).unwrap(), patch);
        for (source, expected) in [("¤", "ä"), ("see → here", "see ⇒ here")] {
            let patch = create_source_patch(source, expected, SourceEditKind::Refactor, "unicode");
            assert_eq!(apply_source_patch(source, &patch).unwrap(), expected);
        }
    }
}
