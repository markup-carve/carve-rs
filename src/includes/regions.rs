use super::*;

pub(super) fn source_lines(source: &str) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if matches!(bytes[i], b'\r' | b'\n') {
            lines.push(&source[start..i]);
            if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            start = i + 1;
        }
        i += 1;
    }
    if start < source.len() {
        lines.push(&source[start..]);
    }
    lines
}

/// Physical lines in a file. A leading U+FEFF is not content, so a file
/// holding only one has no lines.
pub(super) fn content_line_count(source: &str) -> usize {
    source_lines(source.strip_prefix('\u{FEFF}').unwrap_or(source)).len()
}

pub(super) fn slice_lines(source: &str, range: (usize, usize)) -> String {
    let lines = source_lines(source);
    let start = range.0.saturating_sub(1);
    let end = range.1.min(lines.len());
    if start >= end {
        return String::new();
    }
    lines[start..end].join("\n")
}

/// Which marker set a region lookup uses (spec I1b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeRegionMode {
    /// A Carve include: only `%% #region` / `%% #endregion` are markers.
    Carve,
    /// A `@code` include: the source-comment forms are markers too.
    Code,
}

/// The 1-based, inclusive content lines of a region. The opening marker is on
/// `start - 1`; an empty region has `end == start - 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncludeRegion {
    /// First content line.
    pub start: usize,
    /// Last content line.
    pub end: usize,
}

/// The region is missing, or a marker in the file is malformed or unpaired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncludeRegionError;

impl IncludeRegionError {
    /// The warning rule expansion reports for this failure.
    pub fn rule(&self) -> &'static str {
        "include-region-invalid"
    }
}

/// Resolve `@region:name` in a file's text exactly as [`expand_includes`] does,
/// so a host (an editor jumping to a region) follows the engine's marker rules.
pub fn select_include_region(
    source: &str,
    name: &str,
    mode: IncludeRegionMode,
) -> Result<IncludeRegion, IncludeRegionError> {
    region_range(source, name, matches!(mode, IncludeRegionMode::Code))
        .map(|(start, end)| IncludeRegion { start, end })
        .ok_or(IncludeRegionError)
}

pub(super) fn region_range(source: &str, selected: &str, code: bool) -> Option<(usize, usize)> {
    // I1b: the file's leading U+FEFF is not content, so it cannot hide a line-1 marker.
    let source = source.strip_prefix('\u{FEFF}').unwrap_or(source);
    let mut seen = HashSet::new();
    let mut stack: Vec<(&str, usize)> = Vec::new();
    let mut range = None;
    for (index, raw_line) in source_lines(source).into_iter().enumerate() {
        let line = raw_line.trim_start_matches([' ', '\t']);
        let (body, closer) = if line.starts_with("%%") && !line.starts_with("%%%") {
            (&line[2..], "")
        } else if !code {
            continue;
        } else if line.starts_with("//") || line.starts_with("--") {
            (&line[2..], "")
        } else if let Some(body) = line.strip_prefix("/*") {
            (body, "*/")
        } else if let Some(body) = line.strip_prefix("<!--") {
            (body, "-->")
        } else {
            (line, "")
        };
        let body = body.trim_start_matches([' ', '\t']);
        let (closing, rest) = if let Some(rest) = body.strip_prefix("#endregion") {
            (true, rest)
        } else if let Some(rest) = body.strip_prefix("#region") {
            (false, rest)
        } else {
            continue;
        };
        if rest
            .as_bytes()
            .first()
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
        {
            continue;
        }
        let rest = rest.trim_end_matches([' ', '\t']);
        let rest = if closer.is_empty() {
            rest
        } else {
            rest.strip_suffix(closer)?.trim_end_matches([' ', '\t'])
        };
        let name = if rest.is_empty() {
            ""
        } else {
            let name = rest
                .strip_prefix([' ', '\t'])?
                .trim_start_matches([' ', '\t']);
            if name.is_empty()
                || !name.chars().next().is_some_and(is_ident_start)
                || !name.chars().all(is_ident_rest)
            {
                return None;
            }
            name
        };
        if closing {
            let (open_name, open_index) = stack.pop()?;
            if !name.is_empty() && name != open_name {
                return None;
            }
            if open_name == selected {
                range = Some((open_index + 2, index));
            }
        } else {
            if name.is_empty() || !seen.insert(name) {
                return None;
            }
            stack.push((name, index));
        }
    }
    if stack.is_empty() {
        range
    } else {
        None
    }
}
