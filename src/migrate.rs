/// Insert explicit paragraph boundaries that Carve 0.1 inferred from block
/// openers. The transform is idempotent and preserves the source line ending.
pub fn migrate_0_1_to_0_2(source: &str) -> String {
    let eol = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let had_final_eol = source.ends_with('\n');
    let normalized = source.replace("\r\n", "\n");
    let mut lines: Vec<&str> = normalized.split('\n').collect();
    if had_final_eol {
        lines.pop();
    }
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut opaque: Option<(char, usize)> = None;
    let mut paragraph_open = false;
    let mut attachment: Option<(String, usize)> = None;
    let mut colon_widths: Vec<usize> = Vec::new();

    for (index, raw) in lines.iter().enumerate() {
        let (prefix, line) = quote_prefix(raw).unwrap_or(("", raw.trim_start_matches([' ', '\t'])));
        if let Some((ch, width)) = opaque {
            out.push(raw.to_string());
            let trimmed = line.trim_end_matches([' ', '\t']);
            if trimmed.len() >= width && trimmed.chars().all(|c| c == ch) {
                opaque = None;
            }
            continue;
        }
        if line.trim().is_empty() {
            out.push(raw.to_string());
            paragraph_open = false;
            attachment = None;
            continue;
        }

        let code_fence = fence_run(line.trim_start(), &['`', '~']);
        let colon = colon_fence(line.trim_start());
        let colon_closer =
            colon.is_some_and(|(width, labelled)| !labelled && colon_widths.last() == Some(&width));
        let fence_closes = code_fence.is_some_and(|(ch, width)| {
            lines[index + 1..].iter().any(|candidate| {
                let body = quote_prefix(candidate)
                    .map_or(*candidate, |(_, rest)| rest)
                    .trim_start_matches([' ', '\t'])
                    .trim_end_matches([' ', '\t']);
                body.len() >= width && body.chars().all(|c| c == ch)
            })
        });
        let previous_is_continuation = out.last().is_some_and(|previous| {
            let body = quote_prefix(previous)
                .map_or(previous.as_str(), |(_, rest)| rest)
                .trim();
            body == "+"
                || body == ":  +"
                || body
                    .strip_suffix('+')
                    .is_some_and(|prefix| is_list_marker(prefix.trim_end()))
        });
        if !colon_closer
            && (fence_closes || is_old_interrupter(line))
            && paragraph_open
            && !previous_is_continuation
            && out
                .last()
                .is_some_and(|previous| !is_structural_blank(previous))
        {
            out.push(if prefix.is_empty() {
                let current_indent = raw.len() - raw.trim_start_matches([' ', '\t']).len();
                attachment
                    .as_ref()
                    .filter(|(_, content_col)| current_indent < *content_col)
                    .map_or_else(String::new, |(marker, _)| marker.clone())
            } else {
                prefix.trim_end().to_string()
            });
        }
        out.push(raw.to_string());

        let trimmed = line.trim_start();
        if colon_closer {
            colon_widths.pop();
            paragraph_open = false;
            attachment = None;
        } else if let Some((width, _)) = colon {
            colon_widths.push(width);
            paragraph_open = false;
            attachment = None;
        } else if let Some((ch, width)) = code_fence.filter(|_| fence_closes) {
            opaque = Some((ch, width));
            paragraph_open = false;
            attachment = None;
        } else if let Some((ch, width)) = fence_run(trimmed, &['%']) {
            if width >= 3 {
                opaque = Some((ch, width));
            }
            paragraph_open = false;
            attachment = None;
        } else if is_old_interrupter(line) {
            paragraph_open = line.starts_with("> ") && line[2..].trim().len() > 0;
            attachment = paragraph_open.then(|| ("+".to_string(), 2));
        } else {
            if !paragraph_open {
                attachment = attachment_marker_for(raw);
            }
            paragraph_open = line.trim() != "+";
            if !paragraph_open {
                attachment = None;
            }
        }
    }

    let mut result = out.join(eol);
    if had_final_eol {
        result.push_str(eol);
    }
    result
}

fn quote_prefix(raw: &str) -> Option<(&str, &str)> {
    let bytes = raw.as_bytes();
    let mut i = 0;
    let mut found = false;
    loop {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
            i += 1;
        }
        if bytes.get(i) != Some(&b'>') {
            break;
        }
        found = true;
        i += 1;
        if bytes.get(i) == Some(&b' ') {
            i += 1;
        }
    }
    found.then(|| (&raw[..i], &raw[i..]))
}

fn is_structural_blank(line: &str) -> bool {
    quote_prefix(line).map_or(line.trim().is_empty(), |(_, rest)| rest.trim().is_empty())
}

fn fence_run(line: &str, allowed: &[char]) -> Option<(char, usize)> {
    let ch = line.chars().next()?;
    if !allowed.contains(&ch) {
        return None;
    }
    let width = line
        .chars()
        .take_while(|candidate| *candidate == ch)
        .count();
    (width >= 3).then_some((ch, width))
}

fn is_list_marker(prefix: &str) -> bool {
    if matches!(prefix, "-" | "*") {
        return true;
    }
    let marker = prefix.trim_end_matches(['.', ')']);
    !marker.is_empty() && marker.chars().all(|ch| ch.is_ascii_digit())
}

fn colon_fence(line: &str) -> Option<(usize, bool)> {
    let width = line.chars().take_while(|ch| *ch == ':').count();
    if width < 3 {
        return None;
    }
    let rest = &line[width..];
    if rest.is_empty() || rest.starts_with(' ') {
        Some((width, !rest.trim().is_empty()))
    } else {
        None
    }
}

fn attachment_marker_for(raw: &str) -> Option<(String, usize)> {
    let indent_len = raw.len() - raw.trim_start_matches([' ', '\t']).len();
    let body = &raw[indent_len..];
    let list = body
        .split_once(' ')
        .is_some_and(|(marker, rest)| is_list_marker(marker) && !rest.trim().is_empty());
    let definition = body.starts_with(":  ") || (body.starts_with("[^") && body.contains("]: "));
    (list || definition).then(|| {
        let marker_width = if body.starts_with(":  ") {
            3
        } else {
            body.find(' ').map_or(body.len(), |space| space + 1)
        };
        (
            format!("{}+", &raw[..indent_len]),
            indent_len + marker_width,
        )
    })
}

fn is_old_interrupter(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let heading = (1..=6).any(|n| trimmed.starts_with(&format!("{} ", "#".repeat(n))));
    let quote = trimmed == ">" || trimmed.starts_with("> ");
    let thematic = matches!(trimmed.trim_end(), "---" | "***" | "___");
    let table = trimmed.starts_with('|') && trimmed.trim_end().ends_with('|');
    let colon = trimmed.starts_with(":: ") || trimmed.starts_with("::: ") || trimmed == ":::";
    let definition =
        (trimmed.starts_with('[') || trimmed.starts_with("*[")) && trimmed.contains("]: ");
    let comment = trimmed.starts_with("%%");
    let attrs = trimmed.starts_with('{') && trimmed.trim_end().ends_with('}');
    heading || quote || thematic || table || colon || definition || comment || attrs
}

#[cfg(test)]
mod tests {
    use super::migrate_0_1_to_0_2;

    #[test]
    fn inserts_and_is_idempotent() {
        let migrated = migrate_0_1_to_0_2("intro\n# Heading\n");
        assert_eq!(migrated, "intro\n\n# Heading\n");
        assert_eq!(migrate_0_1_to_0_2(&migrated), migrated);
    }
}
