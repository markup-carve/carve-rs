use crate::SPEC_VERSION;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StampForm {
    Line,
    Block,
}

/// Build the marker text (no surrounding blank lines / trailing newline).
pub fn build_marker(generated_by: &str, form: StampForm) -> String {
    match form {
        StampForm::Block => {
            format!("%%%\ncarve-version: {SPEC_VERSION}\ngenerated-by: {generated_by}\n%%%")
        }
        StampForm::Line => {
            format!("%% carve-version: {SPEC_VERSION}; generated-by: {generated_by}")
        }
    }
}

/**
 * Remove a trailing provenance marker (either form) from already-formatted
 * Carve, returning the body with no trailing blank lines. Recognizes the marker
 * by its `carve-version:` first field, so unrelated trailing comments are kept.
 */
pub fn strip_trailing_marker(formatted: &str) -> String {
    let trimmed = formatted.trim_end_matches('\n');
    let mut lines = if trimmed.is_empty() {
        Vec::new()
    } else {
        trimmed.split('\n').collect::<Vec<_>>()
    };

    if let Some(last) = lines.last().copied() {
        if is_line_marker(last) {
            lines.pop();
        } else if is_block_fence(last) {
            let fence = last.trim();
            for i in (0..lines.len().saturating_sub(1)).rev() {
                if lines[i].trim() != fence {
                    continue;
                }
                if lines
                    .get(i + 1)
                    .is_some_and(|line| line.trim().starts_with("carve-version:"))
                {
                    lines.truncate(i);
                }
                break;
            }
        }
    }

    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }

    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

/**
 * Append (or replace) the provenance marker on already-formatted Carve.
 * `generated_by` is the engine identity, e.g. `carve-rs 0.1.0`.
 */
pub fn stamp_carve(formatted: &str, generated_by: &str, form: StampForm) -> String {
    let body = strip_trailing_marker(formatted);
    let marker = build_marker(generated_by, form);
    if body.is_empty() {
        return format!("{marker}\n");
    }
    format!("{}\n\n{marker}\n", body.trim_end_matches('\n'))
}

/// A document's provenance, as recorded by the marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    /// The spec version the document was last processed under.
    pub version: String,
    /// The engine that wrote the marker, when it recorded one.
    pub generated_by: Option<String>,
}

/**
 * Read a document's provenance marker, or `None` when it carries none.
 *
 * Recognizes both documented forms and identifies the marker by
 * `carve-version:` as its first field, so an ordinary trailing comment is not
 * mistaken for provenance. A missing `generated-by` is tolerated.
 *
 * `None` is the normal answer for hand-written documents: nothing has stamped
 * them yet.
 */
pub fn read_stamp(source: &str) -> Option<Stamp> {
    let trimmed = source.trim_end_matches('\n');
    if trimmed.is_empty() {
        return None;
    }
    let lines: Vec<&str> = trimmed.split('\n').collect();
    let last = lines.last().copied()?.trim();

    // A bare `%%%` fence also starts with `%%`, so the block form is checked
    // first - otherwise the line-form branch swallows it and reports no marker.
    if !is_block_fence(last) {
        let rest = last.strip_prefix("%%")?.trim_start_matches([' ', '\t']);
        if let Some(fields) = rest.strip_prefix("carve-version:") {
            let mut parts = fields.splitn(2, ';');
            let version = parts.next().unwrap_or_default().trim();
            if version.is_empty() {
                return None;
            }
            let generated_by = parts
                .next()
                .and_then(|part| part.trim().strip_prefix("generated-by:").map(str::trim))
                .filter(|value| !value.is_empty())
                .map(str::to_string);

            return Some(Stamp {
                version: version.to_string(),
                generated_by,
            });
        }

        return None;
    }

    // Block form: the closing fence is last, the fields sit above it.
    let mut version = None;
    let mut generated_by = None;
    for line in lines.iter().rev().skip(1) {
        let line = line.trim();
        if is_block_fence(line) {
            break;
        }
        if let Some(value) = line.strip_prefix("carve-version:") {
            version = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("generated-by:") {
            generated_by = Some(value.trim().to_string());
        }
    }

    version.map(|version| Stamp {
        version,
        generated_by,
    })
}

/**
 * Whether a document was last processed under an older spec version than this
 * implementation targets, so its `[behavior]` changelog entries are worth
 * reviewing.
 *
 * An unstamped document answers `true`: its provenance is unknown, and assuming
 * it is current is the unsafe direction. A document stamped with a FUTURE
 * version answers `false` - this engine has nothing to say about changes it does
 * not know.
 */
pub fn needs_review(source: &str, current_version: &str) -> bool {
    match read_stamp(source) {
        None => true,
        Some(stamp) => {
            compare_versions(&stamp.version, current_version) == std::cmp::Ordering::Less
        }
    }
}

/// Numeric-segment comparison; a non-numeric segment compares as 0.
fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let mut left = left.split('.');
    let mut right = right.split('.');

    loop {
        match (left.next(), right.next()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (a, b) => {
                let a: u64 = a.unwrap_or("0").parse().unwrap_or(0);
                let b: u64 = b.unwrap_or("0").parse().unwrap_or(0);
                if a != b {
                    return a.cmp(&b);
                }
            }
        }
    }
}

fn is_line_marker(line: &str) -> bool {
    line.strip_prefix("%%").is_some_and(|rest| {
        rest.trim_start_matches([' ', '\t'])
            .starts_with("carve-version:")
    })
}

fn is_block_fence(line: &str) -> bool {
    let trimmed = line.trim_end_matches([' ', '\t']);
    trimmed.len() >= 3 && trimmed.bytes().all(|byte| byte == b'%')
}
