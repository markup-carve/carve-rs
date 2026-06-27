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
