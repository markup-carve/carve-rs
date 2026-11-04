//! Block brace runs Djot refuses for a continuation at or below the `{` column
//! pass through verbatim: the Carve reader refuses them from the same columns
//! (carve#5080).

use super::*;

pub(super) struct ProtectedBraceRuns {
    pub(super) source: String,
    runs: Vec<ProtectedRun>,
}

struct ProtectedRun {
    token: String,
    value: String,
    prefix: String,
    inserted_blank: bool,
}

impl ProtectedBraceRuns {
    pub(super) fn restore(&self, mut output: String) -> String {
        for run in &self.runs {
            if run.inserted_blank {
                output = output.replace(
                    &format!("\n\n{}{}", run.prefix, run.token),
                    &format!("\n{}{}", run.prefix, run.token),
                );
            }
            output = output.replace(&run.token, &run.value);
        }
        output
    }
}

/// The end of a line's Djot quote and list-marker prefix.
fn structural_prefix_end(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut at = 0;
    let spaces = |at: &mut usize| {
        while matches!(bytes.get(*at), Some(b' ' | b'\t')) {
            *at += 1;
        }
    };
    loop {
        spaces(&mut at);
        if bytes.get(at) != Some(&b'>') {
            break;
        }
        at += 1;
    }
    spaces(&mut at);
    loop {
        let start = at;
        let mut end = at;
        if matches!(bytes.get(end), Some(b'-' | b'*' | b'+')) {
            end += 1;
        } else {
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            if end == start || !matches!(bytes.get(end), Some(b'.' | b')')) {
                break;
            }
            end += 1;
        }
        if !matches!(bytes.get(end), Some(b' ' | b'\t')) {
            break;
        }
        at = end;
        spaces(&mut at);
        if bytes.get(at) == Some(&b'[')
            && matches!(bytes.get(at + 1), Some(b' ' | b'x' | b'X' | b'-'))
            && bytes.get(at + 2) == Some(&b']')
            && matches!(bytes.get(at + 3), Some(b' ' | b'\t'))
        {
            at += 3;
            spaces(&mut at);
        }
    }
    at
}

pub(super) fn protect_block_brace_runs(source: &str) -> ProtectedBraceRuns {
    let unchanged = || ProtectedBraceRuns {
        source: source.to_string(),
        runs: Vec::new(),
    };
    if !source.contains('{') || !source.contains('\n') {
        return unchanged();
    }
    let lines: Vec<&str> = source.split('\n').collect();
    let mut inside_image_label = Vec::with_capacity(lines.len());
    let mut label_depth = 0usize;
    for line in &lines {
        inside_image_label.push(label_depth > 0);
        let bytes = line.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            match bytes[at] {
                b'\\' => at += 1,
                b'[' if label_depth > 0 => label_depth += 1,
                b']' if label_depth > 0 => label_depth -= 1,
                b'!' if label_depth == 0 && bytes.get(at + 1) == Some(&b'[') => {
                    label_depth = 1;
                    at += 1;
                }
                _ => {}
            }
            at += 1;
        }
    }
    let mut serial = 0usize;
    while source.contains(&format!("DJOTBRACELITERAL{serial}Q")) {
        serial += 1;
    }
    let mut output: Vec<String> = Vec::with_capacity(lines.len());
    let mut runs = Vec::new();
    let mut offset = 0;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let at = structural_prefix_end(line);
        let start = offset + at;
        let attributes = (!inside_image_label[i]
            && line.as_bytes().get(at) == Some(&b'{')
            && line.as_bytes().get(at + 1) != Some(&b'%'))
        .then(|| read_djot_attributes_unchecked(source, start))
        .flatten()
        .filter(|&end| source[start..end].contains('\n'))
        .filter(|_| !comment_continues_paragraph(source, start));
        let Some(end) = attributes else {
            output.push(line.to_string());
            offset += line.len() + 1;
            i += 1;
            continue;
        };
        let end_line = i + source[start..end].matches('\n').count();
        if lines[i + 1..=end_line]
            .iter()
            .all(|next| structural_prefix_end(next) > at)
        {
            output.push(line.to_string());
            offset += line.len() + 1;
            i += 1;
            continue;
        }
        let token = format!("DJOTBRACELITERAL{serial}Q{}Z", runs.len());
        let inserted_blank = i > 0 && !lines[i - 1].trim().is_empty();
        if inserted_blank {
            output.push(String::new());
        }
        output.push(format!("{}{token}", &line[..at]));
        runs.push(ProtectedRun {
            value: lines[i..=end_line].join("\n")[at..].to_string(),
            prefix: line[..at].to_string(),
            token,
            inserted_blank,
        });
        for taken in &lines[i..=end_line] {
            offset += taken.len() + 1;
        }
        i = end_line + 1;
    }
    if runs.is_empty() {
        return unchanged();
    }
    ProtectedBraceRuns {
        source: output.join("\n"),
        runs,
    }
}
