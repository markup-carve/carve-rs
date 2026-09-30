//! Migrate Djot source to Carve source.
//!
//! Port of carve-php `DjotToCarve`, which is itself the converter form of the
//! carve-js `djot-migrate` linter - the canonical list of Djot/Carve delimiter
//! collisions.
//!
//! Several inline delimiters mean different things in the two languages, so a
//! Djot document fed to a Carve processor renders WRONG WITH NO ERROR. This
//! rewrites exactly those:
//!
//! | Djot | Carve | why |
//! |---|---|---|
//! | `_x_` | `/x/` | Djot emphasis is underline in Carve |
//! | `~x~` | `{,x,}` | Djot subscript is strikethrough in Carve |
//! | `^x^` | `{^x^}` | Carve has no bare superscript; the braced form is the only one |
//! | `**x**` | `*x*` | Markdown bold; Carve bold is a single `*` |
//! | `~~x~~` | `~x~` | Markdown strikethrough; Carve strike is a single `~` |
//! | `{=x=}` | `{=x=}` | already the same braced form |
//!
//! Constructs that mean the same in both languages - `$math$`, `{+ins+}`,
//! `{-del-}`, reference links - are left alone. Delimiters inside code, fenced
//! or inline, and inside link destinations are never rewritten.
//!
//! ```
//! assert_eq!(carve::djot_to_carve("_em_ and ~sub~"), "/em/ and {,sub,}");
//! ```

#[path = "djot_emphasis.rs"]
mod emphasis;

use std::collections::HashSet;

use crate::ast::{BlockNode, FigureTarget};

/// Convert Djot source to Carve source.
pub fn djot_to_carve(djot: &str) -> String {
    let normalized = djot.replace("\r\n", "\n").replace('\r', "\n");
    let (frontmatter, separator, body) = split_frontmatter(&normalized);
    let folded = fold_heading_continuations(body);
    let collapsed = regex::Regex::new(r"(!?\[([^\]\n]*)\])\[\]").unwrap();
    let folded = collapsed.replace_all(&folded, "$1[$2]");
    let mut alt_prefix = "\0DJOTALT\0".to_string();
    while folded.contains(&alt_prefix) {
        alt_prefix.push('\0');
    }
    let mut alts = Vec::new();
    let image = regex::Regex::new(r"!\[([^\]\n]*)\]([\[(])").unwrap();
    let mask = mask_code_and_destinations(&folded);
    let folded = image
        .replace_all(&folded, |caps: &regex::Captures<'_>| {
            let at = caps.get(0).unwrap().start();
            if mask.as_bytes()[at] != b'!'
                || is_escaped(folded.as_bytes(), at)
                || caps[1].contains('\\')
            {
                return caps[0].to_string();
            }
            if !caps[1].bytes().any(|byte| b"_*`{^~".contains(&byte)) {
                alts.push(caps[1].to_string());
                return format!("![{alt_prefix}{}\0]{}", alts.len() - 1, &caps[2]);
            }
            let label = &caps[1];
            alts.push(
                crate::to_plain_text_with_options(
                    &djot_to_carve(&format!("DJOTALT {label} DJOTEND")),
                    &crate::Options {
                        smart_typography: crate::SmartTypographyMode::Source,
                        ..crate::Options::default()
                    },
                )
                .trim_end_matches('\n')
                .strip_prefix("DJOTALT ")
                .unwrap_or("")
                .strip_suffix(" DJOTEND")
                .unwrap_or("")
                .to_string(),
            );
            format!("![{alt_prefix}{}\0]{}", alts.len() - 1, &caps[2])
        })
        .into_owned();
    let (held, prefix, mut spans) = protect_attributed_strong(&folded);
    let words = protect_attributed_words(&held, &prefix, &mut spans);
    let mut empty_term = "\0DJOTEMPTYTERM\0".to_string();
    while words.contains(&empty_term) {
        empty_term.push('\0');
    }
    let converted = rewrite_djot_body(&convert_definition_lists(&words, &empty_term))
        .replace(&empty_term, "%%");
    let restore = regex::Regex::new(&format!(r"{}([0-9]+)\x00", regex::escape(&prefix))).unwrap();
    let converted = restore
        .replace_all(&converted, |caps: &regex::Captures<'_>| {
            spans[caps[1].parse::<usize>().unwrap()].clone()
        })
        .into_owned();

    let alt_restore =
        regex::Regex::new(&format!(r"{}([0-9]+)\x00", regex::escape(&alt_prefix))).unwrap();
    let converted = alt_restore
        .replace_all(&converted, |caps: &regex::Captures<'_>| {
            alts[caps[1].parse::<usize>().unwrap()].clone()
        })
        .into_owned();
    if frontmatter.is_empty() {
        converted
    } else if converted.is_empty() {
        format!("{frontmatter}{separator}")
    } else {
        format!("{}{}{}", frontmatter, separator, converted)
    }
}

fn quote_djot_attribute(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn read_djot_word_attributes(source: &str, start: usize) -> Option<(usize, String)> {
    let bytes = source.as_bytes();
    let mut parts = Vec::new();
    let mut i = start + 1;
    while i < bytes.len() {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
            if bytes[i] == b'\n' && blank_line_follows(bytes, i) {
                return None;
            }
            i += 1;
        }
        if bytes.get(i) == Some(&b'}') {
            return (!parts.is_empty()).then(|| (i + 1, format!("{{{}}}", parts.join(" "))));
        }
        if bytes.get(i) == Some(&b'%') {
            let mut end = i + 1;
            while end < bytes.len() && !matches!(bytes[end], b'%' | b'}') {
                end += 1;
            }
            if end == bytes.len()
                || source[i..end]
                    .match_indices('\n')
                    .any(|(at, _)| blank_line_follows(bytes, i + at))
            {
                return None;
            }
            i = if bytes[end] == b'%' { end + 1 } else { end };
            continue;
        }
        let bare_end = |from: usize| {
            let mut end = from;
            for ch in source[from..].chars() {
                if ch.is_whitespace() || "{}%\"'=<>".contains(ch) {
                    break;
                }
                end += ch.len_utf8();
            }
            end
        };
        if matches!(bytes.get(i), Some(b'#' | b'.')) {
            let kind = bytes[i] as char;
            i += 1;
            let from = i;
            i = bare_end(i);
            if i == from {
                return None;
            }
            let value = &source[from..i];
            let identifier = value
                .bytes()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == b'_' || ch == b'-')
                && value.as_bytes()[0] != b'-';
            parts.push(if identifier {
                format!("{kind}{value}")
            } else {
                format!(
                    "{}={}",
                    if kind == '#' { "id" } else { "class" },
                    quote_djot_attribute(value)
                )
            });
        } else {
            let key_start = i;
            if !bytes.get(i).is_some_and(|ch| ch.is_ascii_alphabetic()) {
                return None;
            }
            i += 1;
            while bytes
                .get(i)
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'_' | b'-'))
            {
                i += 1;
            }
            if bytes.get(i) != Some(&b'=') {
                return None;
            }
            i += 1;
            let key = &source[key_start..i];
            let from = i;
            if bytes.get(i) == Some(&b'"') {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if matches!(bytes[i], b'\n' | b'\r') {
                        return None;
                    }
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                if bytes.get(i) != Some(&b'"') {
                    return None;
                }
                i += 1;
                parts.push(format!("{key}{}", &source[from..i]));
            } else {
                i = bare_end(i);
                if i == from {
                    return None;
                }
                parts.push(format!("{key}{}", quote_djot_attribute(&source[from..i])));
            }
        }
        if i < bytes.len() {
            let next = source[i..].chars().next()?;
            if !next.is_whitespace() && next != '}' && next != '%' {
                return None;
            }
        }
    }
    None
}

fn protect_attributed_words(source: &str, prefix: &str, spans: &mut Vec<String>) -> String {
    let masked = mask_code_and_destinations(source);
    let bytes = source.as_bytes();
    let mut output = String::new();
    let mut cursor = 0;
    let mut i = 0;
    let last_close = source.rfind('}');
    let last_delimiters = b"_*~^".map(|ch| source.rfind(ch as char));
    while last_close.is_some_and(|end| i <= end) {
        if bytes[i] != b'{' || masked.as_bytes()[i] != b'{' || is_escaped(bytes, i) {
            i += 1;
            continue;
        }
        let Some((end, attrs)) = read_djot_word_attributes(source, i) else {
            i += 1;
            continue;
        };
        let mut word = i;
        if i > 0 && masked.as_bytes()[i - 1] == bytes[i - 1] && !b"`*_~^]}>".contains(&bytes[i - 1])
        {
            for (at, ch) in source[cursor..i].char_indices().rev() {
                if ch.is_whitespace()
                    || "\"'{}[]`\0>|".contains(ch)
                    || masked.as_bytes()[cursor + at] != bytes[cursor + at]
                {
                    break;
                }
                word = cursor + at;
            }
        }
        if let Some(closer) = bytes.get(end).filter(|ch| b"_*~^".contains(ch)) {
            if let Some(at) = (word..i)
                .rev()
                .find(|at| bytes[*at] == *closer && !is_escaped(bytes, *at))
            {
                word = at + 1;
            }
        } else if word < i
            && b"_*~^".contains(&bytes[word])
            && last_delimiters[b"_*~^".iter().position(|ch| *ch == bytes[word]).unwrap()]
                .is_some_and(|at| at >= end)
        {
            word = i;
        }
        if word > 0
            && bytes[word - 1] == b'{'
            && bytes.get(word).is_some_and(|ch| b"+-=".contains(ch))
        {
            word += 1;
        }
        if word < i {
            output.push_str(&source[cursor..word]);
            output.push_str(&format!("{prefix}{}\0", spans.len()));
            let converted = rewrite_djot_body(&format!("x {}", &source[word..i]));
            let body = &converted[2..];
            spans.push(format!(
                "[{}{body}]{attrs}",
                if body.starts_with('^') { "\\" } else { "" }
            ));
            cursor = end;
        }
        i = end;
    }
    output.push_str(&source[cursor..]);
    output
}

fn fold_heading_continuations(source: &str) -> String {
    static BLOCK: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let block = BLOCK.get_or_init(|| regex::Regex::new(
        r"^(?:[#>|{]|[-*+][ \t]|[0-9]+[.)][ \t]|:[ \t]|:{2,}|\([0-9a-zA-Z]+\)[ \t]|[`~]{3,}|\^[ \t]|%{3,}|\[[^\]\n]*\]:|(?:\*[ \t]*){3,}$|(?:-[ \t]*){3,}$)"
    ).unwrap());
    let masked = mask_code_and_destinations(source);
    let masks: Vec<_> = masked.split('\n').collect();
    let lines: Vec<_> = source.split('\n').collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let level = line.bytes().take_while(|byte| *byte == b'#').count();
        if (i > 0 && !lines[i - 1].trim().is_empty())
            || !(1..=6).contains(&level)
            || line.as_bytes().get(level) != Some(&b' ')
            || masks[i].as_bytes().first() != Some(&b'#')
            || line[level..].trim().is_empty()
        {
            result.push(line.to_string());
            i += 1;
            continue;
        }
        let prefix = format!("{} ", "#".repeat(level));
        let mut folded = line.to_string();
        i += 1;
        while i < lines.len() {
            if folded
                .bytes()
                .rev()
                .take_while(|byte| *byte == b'\\')
                .count()
                % 2
                == 1
            {
                break;
            }
            let next = lines[i].trim_start_matches([' ', '\t']);
            let part = if let Some(text) = next.strip_prefix(&prefix) {
                if text.trim().is_empty() {
                    break;
                }
                text.trim_start_matches(' ')
            } else {
                if next.trim().is_empty() || block.is_match(next) {
                    break;
                }
                next
            };
            folded.push(' ');
            folded.push_str(part);
            i += 1;
        }
        result.push(folded);
    }
    result.join("\n")
}

fn protect_attributed_strong(source: &str) -> (String, String, Vec<String>) {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = PATTERN.get_or_init(|| regex::Regex::new(r#"\*([^*\n{}]+)(\{(?:\s*(?:[.#][^\s{}"=]+|[\w:-]+=(?:"(?:\\.|[^"\\])*"|[^\s{}"]+)))+\s*\})([^*\n{}]*)\*"#).unwrap());
    let masked = mask_code_and_destinations(source);
    let mut prefix = "\0DJOTSTRONG".to_string();
    while source.contains(&prefix) {
        prefix.push('\0');
    }
    let mut attribute_token = "\0DJOTATTR\0".to_string();
    while source.contains(&attribute_token) {
        attribute_token.push('\0');
    }
    let mut spans = Vec::new();
    let held = pattern
        .replace_all(source, |caps: &regex::Captures<'_>| {
            let whole = caps.get(0).unwrap();
            let start = whole.start();
            let end = whole.end();
            if masked.as_bytes().get(start) != Some(&b'*')
                || masked.as_bytes().get(end - 1) != Some(&b'*')
                || caps[3].ends_with('\\')
                || (start > 0 && matches!(source.as_bytes()[start - 1], b'\\' | b'*'))
                || source.as_bytes().get(end) == Some(&b'*')
                || source[start + 1..]
                    .chars()
                    .next()
                    .is_some_and(char::is_whitespace)
                || source[..end - 1]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace)
            {
                return whole.as_str().to_string();
            }
            let before = &caps[1];
            let word_start = before
                .rfind(|c: char| c.is_whitespace() || "*{}[]`_~^".contains(c))
                .map_or(0, |i| i + before[i..].chars().next().unwrap().len_utf8());
            if word_start == before.len() {
                return whole.as_str().to_string();
            }
            let body = rewrite_djot_body(&format!(
                "{}[{}]{}{}",
                &before[..word_start],
                &before[word_start..],
                attribute_token,
                &caps[3]
            ));
            let span = format!("{{*{}*}}", body.replace(&attribute_token, &caps[2]));
            let key = format!("{prefix}{}\0", spans.len());
            spans.push(span);
            key
        })
        .into_owned();
    (held, prefix, spans)
}

/// Site generators conventionally remove a leading YAML envelope before Djot
/// sees the document. Migration has to do the same: feeding those bytes to a
/// Djot parser turns the opening line into a rule and `_x_` in a YAML scalar
/// into emphasis. The envelope is already valid Carve frontmatter, so keep it
/// byte-for-byte and import only the body.
fn split_frontmatter(source: &str) -> (&str, &str, &str) {
    if !source.starts_with("---\n") {
        return ("", "", source);
    }
    let mut end = 4;
    for line in source[4..].split_inclusive('\n') {
        end += line.len();
        if line.trim_end_matches('\n') == "---" {
            let frontmatter_end = if source.as_bytes().get(end.saturating_sub(1)) == Some(&b'\n') {
                end - 1
            } else {
                end
            };
            let rest = &source[frontmatter_end..];
            let separator_len = if rest.starts_with("\n\n") {
                2
            } else if rest.starts_with('\n') {
                1
            } else {
                0
            };
            return (
                &source[..frontmatter_end],
                &rest[..separator_len],
                &rest[separator_len..],
            );
        }
    }
    ("", "", source)
}

#[derive(Clone, Copy)]
struct DefinitionFrame {
    source: usize,
    target: usize,
    body: bool,
    ready: bool,
}

fn leading_indent(line: &str) -> (usize, usize) {
    let mut columns = 0;
    let mut bytes = 0;
    for byte in line.bytes() {
        match byte {
            b' ' => columns += 1,
            b'\t' => columns += 4 - columns % 4,
            _ => break,
        }
        bytes += 1;
    }
    (columns, bytes)
}

fn bytes_through_columns(line: &str, wanted: usize) -> usize {
    let mut columns = 0;
    let mut bytes = 0;
    for byte in line.bytes() {
        if columns >= wanted {
            break;
        }
        match byte {
            b' ' => columns += 1,
            b'\t' => columns += 4 - columns % 4,
            _ => break,
        }
        bytes += 1;
    }
    bytes
}

fn definition_term(line: &str) -> Option<(usize, usize)> {
    let (columns, mut byte) = leading_indent(line);
    let bytes = line.as_bytes();
    if bytes.get(byte) != Some(&b':') {
        return None;
    }
    byte += 1;
    let whitespace = byte;
    while matches!(bytes.get(byte), Some(b' ' | b'\t')) {
        byte += 1;
    }
    (byte > whitespace && byte < bytes.len()).then_some((columns, byte))
}

/// Translate Djot's definition-item shape without canonicalizing unrelated
/// links, raw blocks, attributes, or author spelling elsewhere in the file.
fn convert_definition_lists(source: &str, empty_term: &str) -> String {
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let masked_source = mask_code_and_destinations(source);
    let masked: Vec<&str> = masked_source.split('\n').collect();
    let mut stack: Vec<DefinitionFrame> = Vec::new();
    for index in 0..lines.len() {
        let raw_term = definition_term(&lines[index]);
        let raw_fence = raw_term.is_some_and(|(_, content)| {
            let payload = &lines[index][content..];
            (payload.starts_with("~~~")
                || (payload.starts_with("```") && !payload.trim_start_matches('`').contains('`')))
                && masked[index].trim_start().starts_with(':')
        });
        if let (Some((indent, _)), Some((raw_indent, content))) = (
            if raw_fence
                || raw_term.is_some_and(|(_, content)| lines[index][content..].starts_with('`'))
            {
                raw_term
            } else {
                definition_term(masked[index])
            },
            raw_term,
        ) {
            if indent != raw_indent {
                continue;
            }
            while stack.last().is_some_and(|frame| indent < frame.source) {
                stack.pop();
            }
            let top = stack.last().copied();
            let may_start = top.is_some() || index == 0 || lines[index - 1].trim().is_empty();
            if may_start {
                let term = lines[index][content..].to_owned();
                if top.is_none() || top.is_some_and(|frame| indent == frame.source) {
                    let target = top.map_or(indent, |frame| frame.target);
                    let starts = top.is_none();
                    if let Some(frame) = stack.last_mut() {
                        frame.body = false;
                        frame.ready = false;
                    } else {
                        stack.push(DefinitionFrame {
                            source: indent,
                            target,
                            body: false,
                            ready: false,
                        });
                    }
                    lines[index] = format!(
                        "{}{}:: {}",
                        if starts {
                            format!("{}{{loose}}\n", " ".repeat(target))
                        } else {
                            String::new()
                        },
                        " ".repeat(target),
                        if raw_fence { empty_term } else { &term }
                    );
                    if raw_fence {
                        lines[index].push_str(&format!("\n{}:  {term}", " ".repeat(target)));
                        let frame = stack.last_mut().unwrap();
                        frame.body = true;
                        frame.ready = true;
                    }
                    continue;
                }
                if top.is_some_and(|frame| frame.ready && indent >= frame.source + 2) {
                    let frame = top.expect("checked above");
                    let target = frame.target + 3;
                    let lead = if frame.body {
                        " ".repeat(target)
                    } else {
                        format!("{}:  ", " ".repeat(frame.target))
                    };
                    if let Some(parent) = stack.last_mut() {
                        parent.body = true;
                    }
                    lines[index] = format!("{lead}{{loose}}\n{}:: {term}", " ".repeat(target));
                    stack.push(DefinitionFrame {
                        source: indent,
                        target,
                        body: false,
                        ready: false,
                    });
                    continue;
                }
            }
        }
        if stack.is_empty() {
            continue;
        }
        if lines[index].trim().is_empty() {
            if let Some(frame) = stack.last_mut() {
                frame.ready = true;
            }
            continue;
        }
        if !stack.last().is_some_and(|frame| frame.ready) {
            continue;
        }
        let (indent, _) = leading_indent(&lines[index]);
        while stack.last().is_some_and(|frame| indent < frame.source + 2) {
            stack.pop();
        }
        let Some(frame) = stack.last_mut() else {
            continue;
        };
        let payload =
            lines[index][bytes_through_columns(&lines[index], frame.source + 2)..].to_owned();
        let extra = indent.saturating_sub(frame.source + 2);
        lines[index] = if frame.body {
            format!("{}{}", " ".repeat(frame.target + 3 + extra), payload)
        } else {
            format!(
                "{}:  {}{}",
                " ".repeat(frame.target),
                " ".repeat(extra),
                payload
            )
        };
        frame.body = true;
    }
    lines.join("\n")
}

fn quote_prefix_len(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut accepted = 0;
    let mut cursor = 0;
    loop {
        let checkpoint = cursor;
        while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'>') {
            return accepted;
        }
        cursor += 1;
        while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
            cursor += 1;
        }
        accepted = cursor;
        if cursor == checkpoint {
            return accepted;
        }
    }
}

fn list_marker(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let thematic: Vec<_> = trimmed
        .bytes()
        .filter(|byte| !matches!(byte, b' ' | b'\t'))
        .collect();
    if thematic.len() >= 3
        && thematic
            .iter()
            .all(|byte| *byte == thematic[0] && matches!(*byte, b'*' | b'-'))
    {
        return false;
    }
    let Some((marker, rest)) = trimmed.split_once([' ', '\t']) else {
        return false;
    };
    if rest.trim().is_empty() {
        return false;
    }
    matches!(marker, "-" | "*" | "+" | ":")
        || marker.strip_suffix(['.', ')']).is_some_and(|value| {
            !value.is_empty() && value.chars().all(|ch| ch.is_ascii_alphanumeric())
        })
        || (marker.starts_with('(')
            && marker.ends_with(')')
            && marker[1..marker.len() - 1]
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric()))
}

fn nested_block(lines: &[&str], line: usize, quote: &str, columns: usize) -> bool {
    for candidate in lines[..line].iter().rev() {
        let Some(candidate) = candidate.strip_prefix(quote) else {
            break;
        };
        if candidate.trim().is_empty() {
            continue;
        }
        let (candidate_columns, _) = leading_indent(candidate);
        if candidate_columns >= columns {
            continue;
        }
        return list_marker(candidate);
    }
    false
}

fn convert_djot_block_markers(source: &str) -> String {
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let masked_source = mask_code_and_destinations(source);
    let masked: Vec<&str> = masked_source.split('\n').collect();
    for index in 0..lines.len() {
        let prefix = quote_prefix_len(masked[index]);
        let quote = &masked[index][..prefix];
        let rest = &masked[index][prefix..];
        let (columns, indent_bytes) = leading_indent(rest);
        let content = &rest[indent_bytes..];
        let nested = nested_block(&masked, index, quote, columns);
        if let Some(close) = content.strip_prefix('(').and_then(|value| value.find(')')) {
            let token = &content[1..close + 1];
            let tail = &content[close + 2..];
            if !token.is_empty()
                && token.chars().all(|ch| ch.is_ascii_alphanumeric())
                && tail.starts_with([' ', '\t'])
                && !tail.trim().is_empty()
            {
                let authored = &lines[index];
                let authored_rest = &authored[prefix + indent_bytes..];
                lines[index] = format!(
                    "{}{}{}.{}",
                    quote,
                    if nested { &rest[..indent_bytes] } else { "" },
                    token,
                    &authored_rest[close + 2..]
                );
                continue;
            }
        }
        let mut marker = None;
        let mut count = 0;
        let mut valid = true;
        for byte in content.bytes() {
            if matches!(byte, b' ' | b'\t') {
                continue;
            }
            if !matches!(byte, b'*' | b'-') || marker.is_some_and(|seen| seen != byte) {
                valid = false;
                break;
            }
            marker = Some(byte);
            count += 1;
        }
        if valid && count >= 3 {
            lines[index] = format!(
                "{}{}***",
                quote,
                if nested { &rest[..indent_bytes] } else { "" }
            );
        }
    }
    lines.join("\n")
}

fn mask_djot_attributes(source: &str) -> String {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        let item = r#"(?:[.#][A-Za-z0-9_][A-Za-z0-9_-]*|[A-Za-z_][A-Za-z0-9_-]*=(?:"(?:\\.|[^"\\\n])*"|[A-Za-z0-9_:-]+))"#;
        regex::Regex::new(&format!(r"^\s*{item}(?:\s+{item})*\s*$")).unwrap()
    });
    let mut masked = mask_code_and_destinations(source).into_bytes();
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut checked = 0;
    let mut block_start = true;
    let mut last_block_end = None;
    while i < bytes.len() {
        if bytes[i] != b'{' || masked[i] != b'{' || is_escaped(bytes, i) {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        let mut quoted = false;
        while end < bytes.len() {
            match bytes[end] {
                b'\n' if quoted || blank_line_follows(bytes, end) => break,
                b'\\' => end += 1,
                b'"' => quoted = !quoted,
                b'{' | b'}' if !quoted => break,
                _ => {}
            }
            end += 1;
        }
        for byte in &bytes[checked..i] {
            if *byte == b'\n' {
                block_start = true;
            } else if !matches!(*byte, b' ' | b'\t') {
                block_start = false;
            }
        }
        checked = i;
        let attached_span =
            i > 0 && bytes[i - 1] == b']' && !bytes[i..end.min(bytes.len())].contains(&b'\n');
        let attached_block = if block_start {
            let after = source.get(end + 1..).unwrap_or("");
            let (line_tail, following) = after.split_once('\n').unwrap_or((after, ""));
            let boundary = match source[..i].rfind('\n') {
                None => true,
                Some(previous_end) => {
                    let previous_start = source[..previous_end].rfind('\n').map_or(0, |at| at + 1);
                    source[previous_start..previous_end].trim().is_empty()
                        || last_block_end
                            .is_some_and(|end| end >= previous_start && end < previous_end)
                }
            };
            boundary
                && line_tail.trim().is_empty()
                && following
                    .split('\n')
                    .next()
                    .is_some_and(|line| !line.trim().is_empty())
        } else {
            false
        };
        if end < bytes.len()
            && bytes[end] == b'}'
            && (attached_span || attached_block)
            && pattern.is_match(&source[i + 1..end])
        {
            for byte in &mut masked[i..=end] {
                if *byte != b'\n' {
                    *byte = 0;
                }
            }
            if attached_block {
                last_block_end = Some(end);
            }
            i = end + 1;
        } else {
            i = end.max(i + 1);
        }
    }
    String::from_utf8(masked).expect("attribute masks preserve UTF-8 boundaries")
}

struct OrphanAttributeSpans {
    prefix: String,
    values: Vec<String>,
}

impl OrphanAttributeSpans {
    fn restore(&self, source: &str) -> String {
        let pattern =
            regex::Regex::new(&format!(r"{}([0-9]+)\x00", regex::escape(&self.prefix))).unwrap();
        pattern
            .replace_all(source, |caps: &regex::Captures<'_>| {
                self.values[caps[1].parse::<usize>().unwrap()].clone()
            })
            .into_owned()
    }
}

fn consume_orphan_djot_attributes(source: &str) -> (String, OrphanAttributeSpans) {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        let item = r#"(?:[.#][A-Za-z0-9_][A-Za-z0-9_-]*|[A-Za-z][A-Za-z0-9_-]*=(?:"(?:\\.|[^"\\\n])*"|[A-Za-z0-9_:-]+))"#;
        regex::Regex::new(&format!(r"\{{[ \t]*{item}(?:[ \t]+{item})*[ \t]*\}}")).unwrap()
    });
    let masked = mask_djot_attributes(source);
    let uri = regex::Regex::new(r"<[A-Za-z][A-Za-z0-9+.-]*:[^<>\s]*>").unwrap();
    let masked = uri.replace_all(&masked, |caps: &regex::Captures<'_>| {
        " ".repeat(caps[0].len())
    });
    let masked_lines: Vec<&str> = masked.split('\n').collect();
    let mut spans = OrphanAttributeSpans {
        prefix: "\0DJOTORPHAN\0".into(),
        values: Vec::new(),
    };
    while source.contains(&spans.prefix) {
        spans.prefix.push('\0');
    }
    let block_end = regex::Regex::new(
        r"^(?:`{3,}|~{3,}|:{3,}|#{1,6} |[-*+] |[0-9]+[.)] |> |:{1,2} |(?:\*[ \t]*){3,}|(?:-[ \t]*){3,}|\|.*\||\[[^\]]+\]:)",
    )
    .unwrap();
    let quote_prefix = regex::Regex::new(r"^(?:[ \t]*>[ \t]*)*[ \t]*").unwrap();
    let marker = regex::Regex::new(
        r"^(?:[ \t]*>[ \t]*)*[ \t]*(?:[-*+]|[0-9]+[.)]|#{1,6}|:{1,2})[ \t]+(?:\[[ xX-]\][ \t]+)?$",
    )
    .unwrap();
    let source_lines: Vec<&str> = source.split('\n').collect();
    let mut lines = Vec::new();
    for (index, line) in source.split('\n').enumerate() {
        let mask = masked_lines[index].as_bytes();
        let bytes = line.as_bytes();
        let first = quote_prefix.find(line).unwrap().end();
        let mut output = String::new();
        let mut cursor = 0;
        let mut drop_line = false;
        for attrs in pattern.find_iter(line) {
            let at = attrs.start();
            if at < cursor || mask[at] != b'{' || is_escaped(bytes, at) {
                continue;
            }
            if at > 0
                && at != cursor
                && (b"]*_}^~".contains(&bytes[at - 1])
                    || (mask[at - 1] == b' ' && bytes[at - 1] != b' '))
            {
                continue;
            }
            if marker.is_match(&pattern.replace_all(&line[..at], ""))
                && pattern.replace_all(&line[at..], "").trim().is_empty()
            {
                continue;
            }
            let alone = at == first && attrs.end() == line.trim_end().len();
            let previous = source_lines
                .get(index.wrapping_sub(1))
                .copied()
                .unwrap_or("");
            let previous = quote_prefix.replace(previous, "");
            let previous = previous.trim();
            if alone
                && source_lines
                    .get(index + 1)
                    .is_some_and(|line| !line.trim().is_empty())
                && (index == 0
                    || previous.is_empty()
                    || (previous.starts_with('{') && previous.ends_with('}'))
                    || block_end.is_match(previous))
            {
                continue;
            }
            drop_line |= alone;
            output.push_str(&line[cursor..at]);
            cursor = attrs.end();
            let end = cursor + line[cursor..].len()
                - line[cursor..].trim_start_matches([' ', '\t']).len();
            let space = &line[cursor..end];
            let value = if at != first {
                space.to_string()
            } else if space.is_empty() {
                String::new()
            } else {
                format!("!`{space}`")
            };
            output.push_str(&format!("{}{}\0", spans.prefix, spans.values.len()));
            spans.values.push(value);
            cursor = end;
        }
        output.push_str(&line[cursor..]);
        if !drop_line {
            lines.push(output);
        }
    }
    (lines.join("\n"), spans)
}

fn emphasis_mask(source: &str) -> String {
    let mut mask = mask_djot_attributes(source).into_bytes();
    let autolink = regex::Regex::new(r"<[^<>\s]+>").unwrap();
    let scheme = regex::Regex::new(r"[^:]@|[A-Za-z]:").unwrap();
    for value in autolink.find_iter(source) {
        if scheme.is_match(value.as_str()) {
            mask[value.range()].fill(b' ');
        }
    }
    for label in regex::Regex::new(r"\[\^[^\]\n]*\]")
        .unwrap()
        .find_iter(source)
    {
        mask[label.range()].fill(b' ');
    }
    let refs = regex::Regex::new(
        r"(?m)^[ \t]*(?:>[ \t]*)*(?:(?:[-*+]|[0-9]+[.)])[ \t]+)?\[[^\^\]\n][^\]\n]*\]:[^\n]*",
    )
    .unwrap();
    let reference_boundary =
        regex::Regex::new(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\{[.#A-Za-z]|\[[^\]]*\]:)").unwrap();
    for value in refs.find_iter(source) {
        let text = value.as_str().trim_start_matches([' ', '\t', '>']);
        if text.starts_with("[^") {
            continue;
        }
        let previous = source[..value.start()]
            .split('\n')
            .rev()
            .nth(1)
            .unwrap_or("")
            .trim_start_matches([' ', '\t', '>'])
            .trim();
        if previous.is_empty() || reference_boundary.is_match(previous) {
            mask[value.range()].fill(b' ');
        }
    }
    let images = regex::Regex::new(r"!\[[^\]\n]*\]").unwrap();
    for value in images.find_iter(source) {
        if !is_escaped(source.as_bytes(), value.start())
            && !is_escaped(source.as_bytes(), value.end() - 1)
        {
            mask[value.range()].fill(b' ');
        }
    }
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' && i > 0 && bytes[i - 1] == b']' {
            if let Some(end) = source[i..].split('\n').next().unwrap().find(']') {
                mask[i..=i + end].fill(b' ');
            }
        }
        i += 1;
    }
    String::from_utf8(mask).expect("inline masks preserve UTF-8 boundaries")
}

fn rewrite_djot_body(djot: &str) -> String {
    let source = convert_djot_block_markers(&djot.replace("\r\n", "\n").replace('\r', "\n"));
    // Before anything else, and deliberately as a same-length rewrite: `+` and
    // `-` are one byte each, so every offset the mask and the rules below
    // compute stays valid. Doing it afterwards would mean re-masking.
    let source = normalize_plus_bullets(&source);
    // Layout, not a delimiter, and it runs before the escape pass because it
    // works on whole lines: a blank-line run Djot reads as nothing is a list
    // boundary in Carve.
    let source = collapse_false_list_boundaries(&source);
    let (source, orphan_spans) = consume_orphan_djot_attributes(&source);
    let mask = emphasis_mask(&source);
    orphan_spans.restore(&emphasis::convert(&source, &mask, rewrite_djot_inline))
}

fn rewrite_djot_inline(source: &str) -> String {
    let masked = emphasis_mask(source);
    let source = escape_plain_carve_syntax_masked(source, HandledDelimiters::DJOT, &masked);
    let masked = emphasis_mask(&source);

    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut taken: Vec<(char, usize, usize)> = Vec::new();

    for rule in RULES {
        for (start, end, inner_start, inner_end) in find_pairs(&masked, rule) {
            // One delimiter run belongs to one rule. A `~~x~~` claimed by the
            // strikethrough rule must not be re-read as two subscripts.
            if taken
                .iter()
                .any(|(family, s, e)| *family == rule.family && start < *e && *s < end)
            {
                continue;
            }
            taken.push((rule.family, start, end));
            // Only the DELIMITERS are replaced, never the inner text, so a
            // construct of another family nested inside this one is still
            // rewritten by its own rule rather than swallowed whole.
            edits.push((start, inner_start, rule.open.to_string()));
            edits.push((inner_end, end, rule.close.to_string()));
        }
    }

    if edits.is_empty() {
        return source;
    }

    edits.sort_by_key(|(start, _, _)| *start);

    let mut out = String::with_capacity(source.len());
    let mut cursor = 0;
    for (start, end, replacement) in edits {
        if start < cursor {
            continue;
        }
        out.push_str(&source[cursor..start]);
        out.push_str(&replacement);
        cursor = end;
    }
    out.push_str(&source[cursor..]);

    out
}

struct Rule {
    /// The delimiter run that opens the construct.
    delimiter: &'static str,
    /// The delimiter run that closes it. Equal to `delimiter` for a symmetric
    /// construct such as `~x~`; different for a braced one, where `{~` opens
    /// and `~}` closes. Searching for the OPENER as the closer is why the
    /// `{=x=}` rule below could never fire - it looked correct only because
    /// its conversion is the identity, so a rule that never matched and a rule
    /// that matched and changed nothing produce the same output.
    closer: &'static str,
    /// Which delimiter character owns the range, so two rules over the same
    /// character cannot both claim it.
    family: char,
    open: &'static str,
    close: &'static str,
    /// `_` only opens and closes at a word boundary. See INTRAWORD below.
    word_bounded: bool,
    /// The inverse: this rule matches ONLY between word characters, which is
    /// the case the word-bounded rule above deliberately declines.
    intraword: bool,
}

/// Longest run first within a family: `~~x~~` is strikethrough, and only what
/// is left over is read as a subscript.
const RULES: &[Rule] = &[
    Rule {
        delimiter: "**",
        closer: "**",
        family: '*',
        open: "*",
        close: "*",
        word_bounded: false,
        intraword: false,
    },
    Rule {
        delimiter: "~~",
        closer: "~~",
        family: '~',
        open: "~",
        close: "~",
        word_bounded: false,
        intraword: false,
    },
    // Djot spells subscript braced as well as bare and means the same by each.
    // The braced form is listed BEFORE the bare one and shares its family, so
    // it claims the range first and the bare rule's match inside the braces is
    // rejected by the overlap check. Converting it as one edit is what keeps
    // the source's own braces from being left behind around a replacement that
    // supplies its own.
    Rule {
        delimiter: "{~",
        closer: "~}",
        family: '~',
        open: "{,",
        close: ",}",
        word_bounded: false,
        intraword: false,
    },
    Rule {
        delimiter: "~",
        closer: "~",
        family: '~',
        open: "{,",
        close: ",}",
        word_bounded: false,
        intraword: false,
    },
    // Braced superscript is spelled identically in both languages, so this is
    // the identity. It still needs a rule: claiming the range is what stops the
    // bare rule below from matching the `^x^` inside the braces and wrapping it
    // a second time into `{{^x^}}`.
    Rule {
        delimiter: "{^",
        closer: "^}",
        family: '^',
        open: "{^",
        close: "^}",
        word_bounded: false,
        intraword: false,
    },
    Rule {
        delimiter: "^",
        closer: "^",
        family: '^',
        open: "{^",
        close: "^}",
        word_bounded: false,
        intraword: false,
    },
    Rule {
        delimiter: "_",
        closer: "_",
        family: '_',
        open: "/",
        close: "/",
        word_bounded: true,
        intraword: false,
    },
    // The complement of the rule above, and it CONVERTS rather than leaving the
    // run literal. The input is a DJOT document: Djot emphasizes an intraword
    // `_`, and an author who wanted the literal characters had to escape them.
    // `snake\_case\_name` renders as `snake_case_name` in Djot and arrives here
    // already escaped, so an UNESCAPED `snake_case_name` is emphasis the author
    // saw in their own renderer and kept.
    //
    // The braced form is required, not stylistic: a bare `/` is literal
    // intraword in Carve, so only `snake{/case/}name` renders as
    // `snake<em>case</em>name`.
    Rule {
        delimiter: "_",
        closer: "_",
        family: '_',
        open: "{/",
        close: "/}",
        word_bounded: false,
        intraword: true,
    },
    Rule {
        delimiter: "{=",
        closer: "=}",
        family: '{',
        open: "{=",
        close: "=}",
        word_bounded: false,
        intraword: false,
    },
];

/// INTRAWORD UNDERSCORES ARE DELIBERATELY LEFT ALONE, and this is a choice
/// about what the author MEANT rather than about what Djot says.
///
/// A strict Djot reader emphasizes them: pandoc's Djot reader turns
/// `snake_case_name` into `snake<em>case</em>name`. This converter does not,
/// because the documents it exists for - notes, READMEs, generated docs - are
/// full of `snake_case` identifiers that no author intended as emphasis, and
/// Carve itself leaves an intraword `_` literal for exactly that reason.
///
/// So the migration is faithful to intent, not to a strict reading, and the
/// cost is real: a Djot document that DID mean emphasis there loses it
/// silently. Documented rather than left as a surprise in the pattern.
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Is the byte at `index` escaped by a backslash? An odd run of backslashes
/// before it escapes it; an even run is literal backslashes and the character
/// still counts. `\_not em\_` is text, not emphasis.
fn is_escaped(bytes: &[u8], index: usize) -> bool {
    let mut backslashes = 0;
    let mut k = index;
    while k > 0 && bytes[k - 1] == b'\\' {
        backslashes += 1;
        k -= 1;
    }

    backslashes % 2 == 1
}

/// Every `(start, end, inner_start, inner_end)` this rule matches, scanning
/// left to right and never overlapping itself.
fn find_pairs(masked: &str, rule: &Rule) -> Vec<(usize, usize, usize, usize)> {
    let bytes = masked.as_bytes();
    let delimiter = rule.delimiter.as_bytes();
    let width = delimiter.len();
    let closer_width = rule.closer.len();
    let mut found = Vec::new();
    let mut i = 0;

    while i + width + closer_width <= bytes.len() {
        if !bytes[i..].starts_with(delimiter) {
            i += 1;
            continue;
        }

        // An escaped delimiter is literal text and opens nothing.
        if is_escaped(bytes, i) {
            i += width;
            continue;
        }

        // A `~~` opener must not be read as the `~` rule's opener plus content,
        // so a longer run of the same character is not an opener for the
        // shorter rule.
        if width == 1 && bytes.get(i + 1) == Some(&delimiter[0]) {
            i += 2;
            continue;
        }

        if rule.word_bounded && i > 0 {
            let before = masked[..i].chars().next_back();
            if before.is_some_and(is_word_char) {
                i += width;
                continue;
            }
        }

        if rule.intraword {
            let before = if i > 0 {
                masked[..i].chars().next_back()
            } else {
                None
            };
            if !before.is_some_and(is_word_char) {
                i += width;
                continue;
            }
        }

        let inner_start = i + width;
        // An opener is not one when whitespace follows it.
        if bytes
            .get(inner_start)
            .is_some_and(|b| b.is_ascii_whitespace())
        {
            i += width;
            continue;
        }

        match find_closer(masked, inner_start, rule) {
            Some(inner_end) => {
                found.push((i, inner_end + closer_width, inner_start, inner_end));
                i = inner_end + closer_width;
            }
            None => i += width,
        }
    }

    found
}

fn find_closer(masked: &str, from: usize, rule: &Rule) -> Option<usize> {
    let bytes = masked.as_bytes();
    let delimiter = rule.closer.as_bytes();
    let width = delimiter.len();
    // Only a symmetric construct can mistake a longer run of its own delimiter
    // for a closer; `~}` cannot be part of a `~~` run.
    let symmetric = rule.closer == rule.delimiter;
    let mut j = from;

    while j + width <= bytes.len() {
        // A construct never spans a blank line: that is a paragraph break, and
        // a delimiter on the far side of one closes nothing.
        if bytes[j] == b'\n' && blank_line_follows(bytes, j) {
            return None;
        }

        if bytes[j..].starts_with(delimiter) {
            if is_escaped(bytes, j) {
                j += width;
                continue;
            }

            // A closer is not one when whitespace precedes it, and an empty
            // pair is not a construct.
            let preceded_by_space = j > from && bytes[j - 1].is_ascii_whitespace();
            if j == from || preceded_by_space {
                j += 1;
                continue;
            }

            if rule.word_bounded {
                let after = masked[j + width..].chars().next();
                if after.is_some_and(is_word_char) {
                    j += width;
                    continue;
                }
            }

            if rule.intraword {
                let after = masked[j + width..].chars().next();
                if !after.is_some_and(is_word_char) {
                    j += width;
                    continue;
                }
            }

            // For a single-character rule the closer must not be part of a
            // longer run, which belongs to the longer rule.
            if symmetric && width == 1 && bytes.get(j + 1) == Some(&delimiter[0]) {
                j += 2;
                continue;
            }

            return Some(j);
        }

        j += 1;
    }

    None
}

/// Is the newline at `index` followed by a line holding nothing but spaces and
/// tabs, i.e. a paragraph break?
fn blank_line_follows(bytes: &[u8], index: usize) -> bool {
    let mut k = index + 1;
    while k < bytes.len() && (bytes[k] == b' ' || bytes[k] == b'\t') {
        k += 1;
    }

    k >= bytes.len() || bytes[k] == b'\n'
}

/// Replace every byte of code and every link destination with a space, so the
/// scan above cannot see a delimiter that is not one. Offsets are preserved, so
/// a match in the mask splices into the original unchanged.
fn mask_code_and_destinations(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut mask: Vec<u8> = bytes.to_vec();
    let opener = regex::Regex::new(r"^([ \t]*)(?:(:[ \t]+|[-*+][ \t]+|[0-9]+[.)][ \t]+))?(`{3,}|~{3,})[ \t]*=?[A-Za-z0-9_+#.-]*[ \t]*$").unwrap();
    let boundary =
        regex::Regex::new(r"^[ \t]*(?:[-*+] |[0-9]+[.)] |:{1,2} |#{1,6} |\{[.#A-Za-z])").unwrap();
    let mut fence: Option<(u8, usize, usize, Option<usize>, usize)> = None;
    let mut previous_block = true;
    let mut offset = 0;
    for raw in source.split_inclusive('\n') {
        let line = raw.trim_end_matches('\n');
        let (depth, content) = quoted(line);
        let prefix_len = line.len() - content.len();
        if fence.is_some_and(|(_, _, _, _, owner_depth)| {
            depth < owner_depth && !content.trim().is_empty()
        }) {
            fence = None;
        }
        let indent = content.len() - content.trim_start_matches([' ', '\t']).len();
        if fence.is_some_and(|(_, _, _, minimum, owner_depth)| {
            minimum.is_some_and(|minimum| indent < minimum)
                && !content.trim().is_empty()
                && depth == owner_depth
        }) {
            fence = None;
        }
        if let Some((ch, run, maximum, _, owner_depth)) = fence {
            blank_out(&mut mask, offset, offset + line.len());
            let candidate = &content[indent..];
            let length = candidate.bytes().take_while(|byte| *byte == ch).count();
            if depth == owner_depth
                && indent <= maximum
                && length >= run
                && candidate[length..].trim_matches([' ', '\t']).is_empty()
            {
                fence = None;
                previous_block = true;
            }
        } else if let Some(open) = opener.captures(content) {
            let marker = open.get(2).map_or("", |value| value.as_str());
            if !marker.starts_with(':') || previous_block {
                let container = (!marker.is_empty()).then_some(open[1].len() + marker.len());
                let start = open.get(3).unwrap().start() + prefix_len;
                fence = Some((
                    open[3].as_bytes()[0],
                    open[3].len(),
                    container.unwrap_or(open[1].len().max(3)),
                    container,
                    depth,
                ));
                blank_out(&mut mask, offset + start, offset + line.len());
            }
        } else {
            previous_block = content.trim().is_empty() || boundary.is_match(content);
        }
        offset += raw.len();
    }
    let mut i = 0;
    while i < bytes.len() {
        if mask[i] != bytes[i] {
            i += 1;
            continue;
        }
        // An inline code span, delimited by a matching backtick run.
        if bytes[i] == b'`' {
            let run = bytes[i..].iter().take_while(|b| **b == b'`').count();
            if let Some(close) = find_backtick_close(bytes, i + run, run) {
                blank_out(&mut mask, i, close + run);
                i = close + run;
                continue;
            }
        }

        // A link or image destination: `](...)`.
        if bytes[i] == b']' && bytes.get(i + 1) == Some(&b'(') {
            if let Some(close) = bytes[i + 2..].iter().position(|b| *b == b')') {
                let end = i + 2 + close + 1;
                blank_out(&mut mask, i + 1, end);
                i = end;
                continue;
            }
        }

        // A footnote reference is one opaque token. In particular, the two
        // carets in adjacent references must never pair as superscript.
        if bytes[i..].starts_with(b"[^") {
            let line = &bytes[i + 2..];
            let width = line
                .iter()
                .position(|byte| *byte == b'\n')
                .unwrap_or(line.len());
            if let Some(close) = line[..width].iter().position(|byte| *byte == b']') {
                let end = i + 2 + close + 1;
                blank_out(&mut mask, i, end);
                i = end;
                continue;
            }
        }

        i += 1;
    }

    String::from_utf8(mask).unwrap_or_else(|_| source.to_string())
}

fn blank_out(mask: &mut [u8], from: usize, to: usize) {
    let end = to.min(mask.len());
    for byte in &mut mask[from..end] {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

fn find_backtick_close(bytes: &[u8], from: usize, run: usize) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let here = bytes[i..].iter().take_while(|b| **b == b'`').count();
            if here == run {
                return Some(i);
            }
            i += here;
            continue;
        }
        i += 1;
    }

    None
}

/// Collapse a blank-line run that Carve alone would read as a list boundary.
///
/// Djot reads ANY run of blank lines between two compatible sibling markers as
/// one list. Carve reads a run of THREE OR MORE as a hard boundary (PART 9 §11
/// N1a) and opens a second list after it. So passing the author's run through
/// splits a list the source never split, and it does it silently: the halves
/// render as `</ul><ul>`, which shows nothing at all for a bullet list, and on
/// an ordered list restarts the numbering.
///
/// A run of three or more blank lines before a list-marker line therefore
/// collapses to ONE blank line, which is how Carve spells what the Djot source
/// said - one loose list. Other runs are left alone: between two paragraphs the
/// count means nothing in either language, and rewriting it would edit layout
/// the author chose for no gain.
///
/// The run must also FOLLOW a list, so the rewrite fires only where the two
/// languages disagree. Blank lines inside a fenced block are that block's
/// content and are skipped.
fn collapse_false_list_boundaries(source: &str) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let fenced = fenced_lines(&lines);

    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let run_start = i;
        let depth = quoted(lines[i]).0;
        while i < lines.len() && !fenced[i] && is_blank(lines[i]) && quoted(lines[i]).0 == depth {
            i += 1;
        }
        let run = i - run_start;
        let above = out.iter().rev().find(|line| !is_blank(line));
        if run >= 3
            && i < lines.len()
            && at_the_same_depth(lines[i], depth, opens_a_list_item)
            && above.is_some_and(|line| at_the_same_depth(line, depth, continues_a_list))
        {
            // The run's own first line, so a quoted run keeps its `>` prefix
            // and an unquoted one stays the empty line it already was.
            out.push(lines[run_start]);
            continue;
        }
        if run > 0 {
            out.extend_from_slice(&lines[run_start..i]);
            continue;
        }
        out.push(lines[i]);
        i += 1;
    }

    out.join("\n")
}

/// A line's block-quote depth and what it holds inside the quote markers. The
/// boundary applies inside a quote as much as outside it, and there a "blank"
/// line is written `>` -- so both the run and the markers around it are read
/// through the prefix rather than off the raw line.
fn quoted(line: &str) -> (usize, &str) {
    let mut rest = line;
    let mut depth = 0;
    loop {
        let trimmed = rest.trim_start_matches([' ', '\t']);
        match trimmed.strip_prefix('>') {
            Some(after) => {
                depth += 1;
                rest = after.strip_prefix(' ').unwrap_or(after);
            }
            // The remainder is NOT trimmed: an item's own indentation is what
            // says the line continues the item, and inside a quote it sits
            // right after the marker.
            None => return (depth, rest),
        }
    }
}

/// Whether a line carries no content, inside whatever quote holds it.
fn is_blank(line: &str) -> bool {
    quoted(line).1.trim().is_empty()
}

/// Whether a line sits at `depth` and its content answers `test`. A marker one
/// quote level away from the run separates nothing the run could join.
fn at_the_same_depth(line: &str, depth: usize, test: fn(&str) -> bool) -> bool {
    let (line_depth, content) = quoted(line);

    line_depth == depth && test(content)
}

/// Which lines sit inside a fenced block, whose blank lines are content.
fn fenced_lines(lines: &[&str]) -> Vec<bool> {
    let mut inside: Vec<bool> = Vec::with_capacity(lines.len());
    let mut open: Option<(char, usize)> = None;
    for line in lines {
        // Through the quote prefix: a fence opened inside a quote holds its
        // blank lines as content exactly like an unquoted one.
        let trimmed = quoted(line).1.trim_start();
        let first = trimmed.chars().next();
        let run = match first {
            Some(c @ ('`' | '~')) => trimmed.chars().take_while(|x| *x == c).count(),
            _ => 0,
        };
        match open {
            Some((fence_char, open_run)) => {
                inside.push(true);
                if run >= open_run && first == Some(fence_char) {
                    open = None;
                }
            }
            None => {
                if run >= 3 {
                    open = Some((first.expect("a run implies a character"), run));
                    inside.push(true);
                } else {
                    inside.push(false);
                }
            }
        }
    }

    inside
}

/// Whether a line is a list-item marker line: a bullet or an ordered marker
/// followed by content. A marker with nothing after it is not an item.
fn opens_a_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    let mut chars = trimmed.chars();
    match chars.next() {
        Some('-' | '*' | '+') => {
            matches!(chars.next(), Some(' ' | '\t')) && !chars.as_str().trim().is_empty()
        }
        Some(c) if c.is_ascii_alphanumeric() => {
            let head: String = trimmed
                .chars()
                .take_while(|x| x.is_ascii_alphanumeric())
                .collect();
            let rest = &trimmed[head.len()..];
            let mut rest = rest.chars();
            matches!(rest.next(), Some('.' | ')'))
                && matches!(rest.next(), Some(' ' | '\t'))
                && !rest.as_str().trim().is_empty()
        }
        _ => false,
    }
}

/// Whether a list is open above the run: the nearest line with content is
/// either a marker line or an item's indented content. Djot has no indented code blocks,
/// so an indented line under a list is that list's content and nothing else.
fn continues_a_list(line: &str) -> bool {
    opens_a_list_item(line) || line.starts_with(' ') || line.starts_with('\t')
}

/// Rewrite a Djot `+` bullet marker to `-`.
///
/// Djot allows `-`, `*` and `+` as bullets. Carve has no `+` bullet: `+` is the
/// list-continuation marker, so a Djot `+` list degrades to a paragraph - the
/// line stops being a list item at all, which is a structural change and not a
/// delimiter one.
///
/// A LONE `+` is left alone. That is the Carve continuation marker itself, and
/// a marker with no content is exactly the form that means it, so rewriting it
/// would break the construct this rule exists to protect.
///
/// Lines inside a fenced block are skipped via the code mask. The rewrite is
/// one byte for one byte, so it runs before the mask is taken for the inline
/// rules and leaves every later offset valid.
fn normalize_plus_bullets(source: &str) -> String {
    let masked = mask_code_and_destinations(source);
    let mask = masked.as_bytes();
    let mut out = source.as_bytes().to_vec();
    let continuation_lines = if masked
        .lines()
        .any(|line| line.trim_start_matches([' ', '\t']).starts_with('+') && line.contains('|'))
    {
        table_continuation_lines(source)
    } else {
        HashSet::new()
    };

    let mut line_start = 0;
    let mut line_number = 1;
    while line_start <= out.len() {
        let line_end = mask[line_start..]
            .iter()
            .position(|b| *b == b'\n')
            .map(|n| line_start + n)
            .unwrap_or(out.len());

        // A masked line is code: its `+` is content, not a marker.
        let mut i = line_start;
        while i < line_end && (mask[i] == b' ' || mask[i] == b'\t') {
            i += 1;
        }

        if i < line_end && mask[i] == b'+' && !continuation_lines.contains(&line_number) {
            // `+ content` is a bullet; a bare `+` (or `+` then only spaces) is
            // the continuation marker and stays.
            let mut j = i + 1;
            let mut spaced = false;
            while j < line_end && (mask[j] == b' ' || mask[j] == b'\t') {
                spaced = true;
                j += 1;
            }
            if spaced && j < line_end {
                out[i] = b'-';
            }
        }

        if line_end >= out.len() {
            break;
        }
        line_start = line_end + 1;
        line_number += 1;
    }

    String::from_utf8(out).expect("one-byte substitution preserves UTF-8")
}

fn table_continuation_lines(source: &str) -> HashSet<usize> {
    fn visit(blocks: &[BlockNode], lines: &mut HashSet<usize>) {
        for block in blocks {
            match block {
                BlockNode::Table(table) => visit_table(table, lines),
                BlockNode::List(list) => {
                    for item in &list.items {
                        visit(&item.children, lines);
                    }
                }
                BlockNode::BlockQuote(node) => visit(&node.children, lines),
                BlockNode::Admonition(node) => visit(&node.children, lines),
                BlockNode::Directive(node) => visit(&node.children, lines),
                BlockNode::Div(node) => visit(&node.children, lines),
                BlockNode::Section(node) => visit(&node.children, lines),
                BlockNode::LineBlock(node) => visit(&node.children, lines),
                BlockNode::DefinitionList(list) => {
                    for item in &list.items {
                        for definition in &item.definitions {
                            visit(&definition.children, lines);
                        }
                    }
                }
                BlockNode::Figure(figure) => match &*figure.target {
                    FigureTarget::Table(table) => visit_table(table, lines),
                    FigureTarget::BlockQuote(node) => visit(&node.children, lines),
                    _ => {}
                },
                BlockNode::FigureGroup(group) => visit(&group.children, lines),
                BlockNode::ExtensionCarrier(extension) => visit(&extension.children, lines),
                _ => {}
            }
        }
    }

    fn visit_table(table: &crate::ast::Table, lines: &mut HashSet<usize>) {
        for row in &table.rows {
            if let Some(pos) = row.pos.clone() {
                lines.extend((pos.start_line + 1)..=pos.end_line);
            }
            for cell in &row.cells {
                if let Some(blocks) = &cell.blocks {
                    visit(blocks, lines);
                }
            }
        }
    }

    let mut lines = HashSet::new();
    let options = crate::Options::default().with_positions(true);
    visit(
        &crate::parse_with_options(source, &options).children,
        &mut lines,
    );
    lines
}

/// Escape Carve inline syntax that is ORDINARY TEXT in Djot.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HandledDelimiters<'a> {
    /// Braced runs (`{X…X}`) the caller's language spells too.
    pub braced: &'a str,
    /// Bare runs (`X…X`) the caller's language spells too.
    pub bare: &'a str,
}

impl HandledDelimiters<'_> {
    /// Djot: the language of `djot_to_carve`.
    pub(crate) const DJOT: HandledDelimiters<'static> = HandledDelimiters {
        braced: "=+-*_^~",
        bare: "~*_",
    };

    /// A language that owns none of these delimiters: HTML and BBCode text.
    ///
    /// The BBCode importer passes this after protecting its tag and literal
    /// spans, because BBCode owns none of Carve's inline delimiters.
    pub(crate) const PLAIN: HandledDelimiters<'static> = HandledDelimiters {
        braced: "",
        bare: "",
    };

    /// Markdown: the `markdown` profile of the shared escaper corpus.
    #[cfg(test)]
    pub(crate) const MARKDOWN: HandledDelimiters<'static> = HandledDelimiters {
        braced: "*_",
        bare: "*_~",
    };

    fn owns_braced(&self, delim: u8) -> bool {
        self.braced.as_bytes().contains(&delim)
    }

    fn owns_bare(&self, delim: u8) -> bool {
        self.bare.as_bytes().contains(&delim)
    }
}

/// Every braced run Carve spells. A caller's handled set is subtracted from
/// this; what is left is what gets frozen.
const BRACED_DELIMITERS: &[u8] = b",/#=+-*_^~";

/// Every bare run Carve spells (PART 4: `/ * _ ~ =`).
const BARE_DELIMITERS: &[u8] = b"/=~*_";

/// Freeze the Carve constructs in `source` that the caller's language leaves as
/// literal text, given the delimiters that language HANDLES itself.
pub(crate) fn escape_plain_carve_syntax(source: &str, handled: HandledDelimiters<'_>) -> String {
    let masked = mask_code_and_destinations(source);
    escape_plain_carve_syntax_masked(source, handled, &masked)
}

fn escape_plain_carve_syntax_masked(
    source: &str,
    handled: HandledDelimiters<'_>,
    masked: &str,
) -> String {
    let mask = masked.as_bytes();
    let mut at: Vec<usize> = Vec::new();

    // `%%` opens a comment at the start of a line or after whitespace. `%%%` is
    // not a comment opener, so it is left alone.
    let mut i = 0;
    while i + 1 < mask.len() {
        if mask[i] == b'%'
            && mask[i + 1] == b'%'
            && mask.get(i + 2) != Some(&b'%')
            && !is_escaped(mask, i)
            && (i == 0 || matches!(mask[i - 1], b' ' | b'\t' | b'\n'))
        {
            at.push(i);
            i += 2;
            continue;
        }
        i += 1;
    }

    // Braced forms whose delimiter this converter does not own.
    for &delim in BRACED_DELIMITERS {
        if handled.owns_braced(delim) {
            continue;
        }
        let mut i = 0;
        while i + 3 < mask.len() {
            if mask[i] != b'{' || mask[i + 1] != delim || is_escaped(mask, i) {
                i += 1;
                continue;
            }
            match find_braced_close(mask, i + 2, delim) {
                Some(end) => {
                    at.push(i);
                    i = end + 2;
                }
                None => {
                    // AN UNCLOSED BRACED OPENER STILL FREEZES. The escaper's
                    // unit is a LINE, but a braced run is not: `a {^x` here and
                    // `y^} b` on the next line render one `<sup>x\ny</sup>`. An
                    // opener left bare therefore lets the NEXT line close it and
                    // turns two lines of literal text into markup, which is the
                    // one failure a line-oriented escaper cannot see from inside
                    // its own line (corpus case `braced-unclosed`). A bare pair
                    // is deliberately NOT treated this way - `bare-unclosed`
                    // pins it unchanged under every profile.
                    if braced_run_opens(mask, i + 2) {
                        at.push(i);
                    }
                    i += 1;
                }
            }
        }
    }

    // Bare pairs: the ones the caller's language does not spell are the author's
    // literal text. Under the Djot profile that is `/` (Carve emphasis) and `=`
    // (Carve highlight), neither of which is Djot syntax.
    for &delim in BARE_DELIMITERS {
        if handled.owns_bare(delim) {
            continue;
        }
        let mut i = 0;
        while i < mask.len() {
            if mask[i] != delim || is_escaped(mask, i) {
                i += 1;
                continue;
            }
            // A leading `{` excludes the run for a delimiter the caller's
            // language spells in BRACED form, and not otherwise; the asymmetry
            // is the point rather than an oversight. Under Djot, `{=x=}` is a
            // highlight in both languages, so the inner `=` is markup that must
            // survive; there is no `{/x/}` that means the same in both, so the
            // `/` inside an escaped brace is literal text and still needs
            // escaping - escaping the brace alone leaves it free to open Carve
            // emphasis and renders `{<em>x</em>}`. Under a profile that owns
            // neither, both get escaped, which is what makes the whole run
            // literal.
            let brace_protects = handled.owns_braced(delim);
            let before_ok = i == 0
                || !(mask[i - 1].is_ascii_alphanumeric()
                    || mask[i - 1] == delim
                    || (brace_protects && mask[i - 1] == b'{'));
            let after = mask.get(i + 1).copied().unwrap_or(b' ');
            if !before_ok || after.is_ascii_whitespace() || after == delim {
                i += 1;
                continue;
            }
            match find_bare_close(mask, i + 1, delim) {
                Some(end) => {
                    at.push(i);
                    i = end + 1;
                }
                None => i += 1,
            }
        }
    }

    let mut i = 0;
    while i < mask.len() {
        if mask[i] != b'#' || is_escaped(mask, i) {
            i += 1;
            continue;
        }
        let before_ok = i == 0 || !mask[i - 1].is_ascii_alphanumeric();
        let opens_tag = mask
            .get(i + 1)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-');
        if before_ok && opens_tag {
            at.push(i);
        }
        i += 1;
    }

    // A MENTION is the tag's sibling and needs the same rule for the same
    // reason: it opens on its own, so nothing downstream neutralizes it.
    // Ported from carve-php#1381, which fixed the same gap there. Djot has no
    // mention either, so prose quoting a framework directive came back as a
    // span that existed nowhere in the source.
    //
    // Mirrors `parse_mention` rather than approximating it: a mention opens on
    // an `@` NOT preceded by an alphanumeric or `_` and followed by a name
    // character. The preceding-character test is what leaves an email address
    // alone, since `foo@bar` has a letter before the `@`.
    let mut i = 0;
    while i < mask.len() {
        if mask[i] != b'@' || is_escaped(mask, i) {
            i += 1;
            continue;
        }
        let before_ok = i == 0 || !(mask[i - 1].is_ascii_alphanumeric() || mask[i - 1] == b'_');
        let opens_mention = mask
            .get(i + 1)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-');
        if before_ok && opens_mention {
            at.push(i);
        }
        i += 1;
    }

    let mut i = 0;
    while i < mask.len() {
        if mask[i] != b':' || is_escaped(mask, i) {
            i += 1;
            continue;
        }
        if i > 0 && (mask[i - 1].is_ascii_alphanumeric() || mask[i - 1] == b'_') {
            i += 1;
            continue;
        }
        let Some(&first) = mask.get(i + 1) else {
            break;
        };
        if !first.is_ascii_alphanumeric() && first != b'+' && first != b'-' {
            i += 1;
            continue;
        }
        let mut len = 1;
        while let Some(&b) = mask.get(i + 1 + len) {
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'+' || b == b'-' {
                len += 1;
            } else {
                break;
            }
        }
        if mask.get(i + 1 + len) == Some(&b':') {
            at.push(i);
            // The whole shortcode is consumed, the way the parser consumes it.
            i += len + 2;
        } else {
            i += 1;
        }
    }

    if at.is_empty() {
        return source.to_string();
    }

    at.sort_unstable();
    at.dedup();
    let bytes = source.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() + at.len());
    let mut cursor = 0;
    for point in at {
        out.extend_from_slice(&bytes[cursor..point]);
        out.push(b'\\');
        cursor = point;
    }
    out.extend_from_slice(&bytes[cursor..]);

    String::from_utf8(out).expect("inserting an ASCII backslash preserves UTF-8")
}

/// Whether a `{X` whose content starts at `from` opens a braced run at all.
///
/// The parser needs non-space content against the delimiter, so `{ ^x^ }` opens
/// nothing and is ordinary text. This is what separates "does not open" from
/// "opens and is never closed": only the second freezes.
fn braced_run_opens(mask: &[u8], from: usize) -> bool {
    mask.get(from).is_some_and(|b| !b.is_ascii_whitespace())
}

/// The offset of the `X}` that closes a `{X` opened before `from`, on the same
/// line, with non-space content between.
fn find_braced_close(mask: &[u8], from: usize, delim: u8) -> Option<usize> {
    if !braced_run_opens(mask, from) {
        return None;
    }
    let mut j = from;
    while j + 1 < mask.len() {
        if mask[j] == b'\n' {
            return None;
        }
        if mask[j] == delim && mask[j + 1] == b'}' && j > from && !mask[j - 1].is_ascii_whitespace()
        {
            return Some(j);
        }
        j += 1;
    }
    None
}

/// The offset of the delimiter closing a bare pair opened before `from`, using
/// the same word boundaries the Carve parser opens on.
fn find_bare_close(mask: &[u8], from: usize, delim: u8) -> Option<usize> {
    let mut j = from;
    while j < mask.len() {
        if mask[j] == b'\n' {
            return None;
        }
        if mask[j] == delim {
            let preceded_by_space = mask[j - 1].is_ascii_whitespace();
            let after = mask.get(j + 1).copied().unwrap_or(b' ');
            if j > from && !preceded_by_space && !after.is_ascii_alphanumeric() && after != delim {
                return Some(j);
            }
            return None;
        }
        j += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_frontmatter_is_preserved_outside_the_djot_reader() {
        let source = "---\nkey: my_long_value\n---\n\nBody.\n";
        assert_eq!(djot_to_carve(source), source);
    }

    #[test]
    fn multiple_footnotes_are_opaque_to_delimiter_conversion() {
        let source =
            "Here's one.[^foo] Here's two.[^bar]\n\n[^foo]: First note.\n\n[^bar]: Second note.\n";
        let carve = djot_to_carve(source);
        assert!(carve.contains("[^foo]"), "{carve}");
        assert!(carve.contains("[^bar]"), "{carve}");
        assert!(carve.contains("First note."), "{carve}");
        assert!(carve.contains("Second note."), "{carve}");
        assert!(!carve.contains("[{^"), "{carve}");
        assert!(!carve.contains("[^}"), "{carve}");
    }

    #[test]
    fn an_unclosed_footnote_token_does_not_mask_later_lines() {
        assert_eq!(
            djot_to_carve("Unclosed [^ then _one_.\n\nLater _two_."),
            "Unclosed [^ then /one/.\n\nLater /two/."
        );
    }

    #[test]
    fn a_definition_document_does_not_rewrite_code_that_looks_like_a_term() {
        let source = ": term\n\n  Body.\n\n```\n:: code\n```\n";
        let carve = djot_to_carve(source);
        assert!(carve.contains("```\n:: code\n```"), "{carve}");
        assert!(!carve.contains("```\n{loose}"), "{carve}");
    }

    #[test]
    fn a_djot_definition_item_becomes_a_carve_definition_item() {
        let source = ": orange\n\n  A citrus fruit.\n";
        let carve = djot_to_carve(source);
        assert!(carve.contains("{loose}"), "{carve}");
        assert!(carve.contains(":: orange"), "{carve}");
        assert!(carve.contains(":  A citrus fruit."), "{carve}");
    }

    #[test]
    fn definition_conversion_is_local_to_documents_with_links_and_raw_blocks() {
        let source = ": term\n\n  Body with [a link](/url).\n\n``` =latex\n\\alpha\n```\n";
        let carve = djot_to_carve(source);
        assert!(carve.contains(":: term"), "{carve}");
        assert!(carve.contains("[a link](/url)"), "{carve}");
        assert!(carve.contains("``` =latex\n\\alpha\n```"), "{carve}");
    }

    #[test]
    fn djot_only_block_markers_take_carve_spellings() {
        assert_eq!(djot_to_carve("* * *"), "***");
        assert_eq!(djot_to_carve("      - - -"), "***");
        assert_eq!(djot_to_carve("(1) one\n(2) two"), "1. one\n2. two");
        assert_eq!(djot_to_carve("(a) one\n(b) two"), "a. one\nb. two");
        assert_eq!(djot_to_carve("- item\n\n  * * *"), "- item\n\n  ***");
    }

    #[test]
    fn emphasis_becomes_the_carve_spelling() {
        assert_eq!(djot_to_carve("_em_ text"), "/em/ text");
    }

    #[test]
    fn subscript_and_superscript_take_the_braced_form() {
        assert_eq!(djot_to_carve("H~2~O"), "H{,2,}O");
        assert_eq!(djot_to_carve("E=mc^2^"), "E=mc{^2^}");
    }

    #[test]
    fn the_markdown_habits_collapse_to_one_delimiter() {
        assert_eq!(djot_to_carve("**strong**"), "*strong*");
        assert_eq!(djot_to_carve("~~gone~~"), "~gone~");
    }

    #[test]
    fn highlight_is_already_the_same_form() {
        assert_eq!(djot_to_carve("{=marked=}"), "{=marked=}");
    }

    /// Djot reads any blank run between two markers as one list; Carve reads
    /// three or more as a hard boundary. Passing the run through therefore
    /// split a list the source never split.
    #[test]
    fn a_blank_run_before_a_sibling_marker_does_not_split_the_list() {
        assert_eq!(
            djot_to_carve("- apples\n\n\n\n\n- oranges\n"),
            "- apples\n\n- oranges\n"
        );
    }

    /// The boundary is not a top-level rule, so neither is the collapse.
    #[test]
    fn the_run_collapses_inside_an_item_too() {
        assert_eq!(
            djot_to_carve("- outer\n  - inner\n\n\n\n\n  - inner2\n"),
            "- outer\n  - inner\n\n  - inner2\n"
        );
    }

    /// Two blank lines are the loose separator in both languages, so there is
    /// nothing to correct.
    #[test]
    fn a_shorter_run_is_left_alone() {
        assert_eq!(
            djot_to_carve("- apples\n\n\n- oranges\n"),
            "- apples\n\n\n- oranges\n"
        );
    }

    /// Djot's own way of writing two lists is a different marker, and it means
    /// the same in Carve. Nothing about it is a false boundary.
    #[test]
    fn a_marker_change_still_separates_two_lists() {
        assert_eq!(
            djot_to_carve("- apples\n\n* oranges\n"),
            "- apples\n\n* oranges\n"
        );
    }

    /// The rewrite fires only where the two languages disagree, which needs a
    /// list open above the run. After a paragraph the run says nothing in
    /// either language and the author's layout is left as written.
    #[test]
    fn a_run_that_follows_no_list_keeps_its_lines() {
        assert_eq!(
            djot_to_carve("paragraph\n\n\n\n\n- apples\n"),
            "paragraph\n\n\n\n\n- apples\n"
        );
    }

    /// A run followed by an item's own indented content continues that item at
    /// any length -- N1a closes nothing -- so both languages already agree.
    #[test]
    fn a_run_before_an_items_content_keeps_its_lines() {
        assert_eq!(
            djot_to_carve("- apples\n\n\n\n\n  still apples\n"),
            "- apples\n\n\n\n\n  still apples\n"
        );
    }

    /// N1a applies inside a container too, and there a blank line is written
    /// `>`, so the run and the markers around it are read through the prefix.
    #[test]
    fn a_quoted_run_collapses_and_keeps_its_prefix() {
        assert_eq!(
            djot_to_carve("> 1. one\n>\n>\n>\n> 2. two\n"),
            "> 1. one\n>\n> 2. two\n"
        );
    }

    /// An item's own indentation is what says the line continues the item, and
    /// inside a quote it sits right after the marker -- so the quote prefix is
    /// stripped and the indentation is not.
    #[test]
    fn a_quoted_run_after_an_items_indented_content_still_collapses() {
        assert_eq!(
            djot_to_carve("> - apples\n>   more apples\n>\n>\n>\n> - oranges\n"),
            "> - apples\n>   more apples\n>\n> - oranges\n"
        );
    }

    /// A fence opened inside a quote holds its blank lines as content exactly
    /// like an unquoted one.
    #[test]
    fn a_run_inside_a_quoted_fence_is_content() {
        let source = "> ```\n> - one\n>\n>\n>\n> - two\n> ```\n";
        assert_eq!(djot_to_carve(source), source);
    }

    /// A marker one quote level away separates nothing the run could join.
    #[test]
    fn a_run_and_a_marker_at_different_depths_are_left_alone() {
        let source = "> - apples\n>\n>\n>\n- oranges\n";
        assert_eq!(djot_to_carve(source), source);
    }

    /// Blank lines inside a fenced block are that block's content.
    #[test]
    fn a_run_inside_a_fence_is_content() {
        let source = "```\ncode\n\n\n\n- not a marker\n```\n";
        assert_eq!(djot_to_carve(source), source);
    }

    /// An intraword `_x_` CONVERTS, to the braced form. Djot emphasizes it, and
    /// an author who wanted the literal characters had to escape them, so an
    /// unescaped run is emphasis the source states rather than an identifier
    /// the converter should protect.
    #[test]
    fn an_intraword_underscore_converts_to_the_braced_form() {
        assert_eq!(
            djot_to_carve("snake_case_name stays"),
            "snake{/case/}name stays"
        );
        assert_eq!(djot_to_carve("MAX_BUFFER_SIZE"), "MAX{/BUFFER/}SIZE");
        assert_eq!(djot_to_carve("a _x_ and y_z_w"), "a /x/ and y{/z/}w");
    }

    /// The other side, and what makes it safe: the escape survives, so an
    /// author who did mean the identifier keeps it.
    #[test]
    fn an_escaped_intraword_underscore_is_left_alone() {
        assert_eq!(djot_to_carve("snake\\_case\\_name"), "snake\\_case\\_name");
    }

    /// BOUND: the word-bounded rule still emits the BARE form, and shapes that
    /// are not an intraword pair at all are untouched. Removing the intraword
    /// rule leaves every row here passing.
    #[test]
    fn the_surrounding_underscore_shapes_are_unchanged() {
        assert_eq!(djot_to_carve("a _x_ b"), "a /x/ b");
        assert_eq!(djot_to_carve("__init__"), "/init/");
        assert_eq!(djot_to_carve("_leading"), "\\_leading");
        assert_eq!(djot_to_carve("[t](/a_b_c)"), "[t](/a_b_c)");
    }

    #[test]
    fn code_is_never_rewritten() {
        assert_eq!(
            djot_to_carve("`_no_ **no**` yes _yes_"),
            "`_no_ **no**` yes /yes/"
        );
        assert_eq!(
            djot_to_carve("``` js\n_no_\n```\n\n_yes_"),
            "``` js\n_no_\n```\n\n/yes/"
        );
    }

    #[test]
    fn a_destination_is_never_rewritten() {
        assert_eq!(
            djot_to_carve("[t](http://e.com/_a_/b) and _yes_"),
            "[t](http://e.com/_a_/b) and /yes/"
        );
    }

    #[test]
    fn a_pair_never_spans_a_paragraph_break() {
        assert_eq!(djot_to_carve("_a\n\nb_"), "\\_a\n\nb\\_");
        assert_eq!(djot_to_carve("_a\nb_"), "/a\nb/");
    }

    #[test]
    fn an_escaped_delimiter_is_literal_text() {
        assert_eq!(djot_to_carve("\\_not em\\_"), "\\_not em\\_");
    }

    /// A tag is the one construct that is not a pair, so escaping an enclosing
    /// brace cannot neutralize it. Djot has no hashtag - pandoc renders
    /// `a #y b` as `<p>a #y b</p>` - so every `#word` became a Carve tag span
    /// (carve-php#1191).
    #[test]
    fn a_hash_does_not_become_a_tag() {
        assert_eq!(djot_to_carve("a #y b"), "a \\#y b");
        assert_eq!(djot_to_carve("a #1 b"), "a \\#1 b");
        assert_eq!(djot_to_carve("{#y#} x"), "\\{\\#y#} x");
    }

    /// A heading is `#` plus a space and is shared with Djot; `a#y` is not a
    /// tag either. Djot character-reference text freezes before Carve reads it.
    #[test]
    fn the_hash_negatives_stay_bare() {
        assert_eq!(djot_to_carve("# Heading"), "# Heading");
        assert_eq!(djot_to_carve("a#y b"), "a#y b");
        assert_eq!(djot_to_carve("a &#8212; b"), "a &\\#8212; b");
        assert_eq!(djot_to_carve("a &#x2014; b"), "a &\\#x2014; b");
    }

    #[test]
    fn constructs_that_mean_the_same_are_untouched() {
        assert_eq!(djot_to_carve("{+add+} and {-cut-}"), "{+add+} and {-cut-}");
    }

    #[test]
    fn nesting_of_different_families_composes() {
        assert_eq!(djot_to_carve("_a **b** c_"), "/a *b* c/");
    }

    /// Djot spells subscript braced as well as bare and means the same by each,
    /// so the braced form converts too. It previously fell through untouched
    /// and stayed a Carve STRIKETHROUGH, which is a different word.
    #[test]
    fn the_braced_subscript_converts_like_the_bare_one() {
        assert_eq!(djot_to_carve("{~y~} a"), "{,y,} a");
        assert_eq!(djot_to_carve("a{~b~}c"), "a{,b,}c");
        assert_eq!(djot_to_carve("{~y~}"), djot_to_carve("~y~"));
    }

    /// The braced superscript is spelled identically in both languages, so the
    /// conversion is the identity. Without a rule claiming the range the bare
    /// rule matched the `^x^` INSIDE the braces and wrapped it again, into
    /// `{{^x^}}`.
    #[test]
    fn the_braced_superscript_is_not_wrapped_twice() {
        assert_eq!(djot_to_carve("{^x^} a"), "{^x^} a");
        // The inner `{,b,}` is literal text in Djot and a SUBSCRIPT in Carve,
        // so it is escaped rather than passed through. Rendering the result
        // gives Djot's `<sup>a{,b,}c</sup>` back.
        assert_eq!(djot_to_carve("{^a{,b,}c^} x"), "{^a\\{,b,}c^} x");
    }

    /// A `+` bullet is a list item in Djot and the CONTINUATION MARKER in
    /// Carve, so leaving it turns the list into a paragraph.
    #[test]
    fn the_plus_bullet_becomes_a_dash() {
        assert_eq!(djot_to_carve("+ one\n+ two\n"), "- one\n- two\n");
        assert_eq!(djot_to_carve("+ a\n  + b\n"), "- a\n  - b\n");
    }

    /// BOUND: a lone `+` IS the Carve continuation marker, and mid-line text is
    /// not a marker at all. Neither moves under this change.
    #[test]
    fn a_lone_plus_and_an_inline_plus_are_left_alone() {
        assert_eq!(djot_to_carve("+\n"), "+\n");
        assert_eq!(djot_to_carve("a + b\n"), "a + b\n");
    }

    #[test]
    fn a_table_continuation_row_is_left_alone() {
        let source = "| a | b |\n|---|---|\n| one | x |\n+ continues here | y |\n";
        assert_eq!(table_continuation_lines(source), HashSet::from([4]));
        assert_eq!(djot_to_carve(source), source);
    }

    #[test]
    fn a_plus_bullet_containing_a_pipe_still_becomes_a_dash() {
        assert_eq!(
            djot_to_carve("A paragraph.\n\n+ a bullet with a | pipe\n"),
            "A paragraph.\n\n- a bullet with a | pipe\n"
        );
    }

    /// Carve syntax that is ordinary text in Djot has to be escaped or the
    /// conversion renders something the source never said. `%%` is the sharpest
    /// case: Carve reads it as a line comment and the line DISAPPEARS.
    #[test]
    fn carve_syntax_that_is_plain_djot_text_is_escaped() {
        assert_eq!(djot_to_carve("%% not a comment\n"), "\\%% not a comment\n");
        assert_eq!(djot_to_carve("/slashes/ x\n"), "\\/slashes/ x\n");
        assert_eq!(djot_to_carve("=marked= x\n"), "\\=marked= x\n");
        assert_eq!(djot_to_carve("{,sub,} x\n"), "\\{,sub,} x\n");
    }

    /// Escaping the brace ALONE is not enough when the delimiter inside it has
    /// a bare Carve form: `\\{/y/}` still renders `{<em>y</em>}`.
    #[test]
    fn a_braced_delimiter_with_a_bare_form_escapes_both() {
        assert_eq!(djot_to_carve("{/y/} x\n"), "\\{\\/y/} x\n");
    }

    /// BOUND, and the reason the rule above is delimiter-specific: `{=x=}` is a
    /// highlight in BOTH languages, so its inner `=` is markup that must
    /// survive. Escaping it would break the one braced form that already works.
    #[test]
    fn the_braced_highlight_keeps_its_inner_delimiter() {
        assert_eq!(djot_to_carve("{=marked=} x"), "{=marked=} x");
    }

    /// BOUND: escaping never reaches code, a destination, or an unpaired
    /// delimiter. None of these move under any of the rules above.
    #[test]
    fn code_destinations_and_unpaired_delimiters_are_untouched() {
        assert_eq!(djot_to_carve("a `_x_` b\n"), "a `_x_` b\n");
        assert_eq!(djot_to_carve("ftp://x/ y\n"), "ftp://x/ y\n");
        assert_eq!(djot_to_carve("a/b/c\n"), "a/b/c\n");
        assert_eq!(djot_to_carve("%%% not\n"), "%%% not\n");
        assert_eq!(djot_to_carve("```\n/code/\n```\n"), "```\n/code/\n```\n");
    }
}

/// The shared escaper corpus (`tests/spec/tests/corpus-escape/`), read directly
/// against the escaper rather than end to end through a converter.
///
/// This lives beside the function rather than in `tests/` because the function
/// is crate-internal: it is one rule with one implementation, not public API.
/// Before markup-carve/carve-rs#995 the only way to reach it from outside was
/// `carve migrate --from djot`, which can only probe inputs that are INERT in
/// Djot - 46 of the corpus's 55, since the other 9 are Djot markup and what
/// came back was the converter doing its job rather than the escaper.
#[cfg(test)]
mod escape_corpus {
    use super::{escape_plain_carve_syntax, HandledDelimiters};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    /// The profiles THIS crate can produce, by the corpus's names.
    ///
    /// The corpus tells an engine to run "every profile its converters can
    /// produce" and skip the rest, the way the render corpus skips a target an
    /// engine does not implement. All three are listed here deliberately: the
    /// handled set is a parameter now, so a profile with no caller is still a
    /// statement this implementation can be held to, and `markdown` and `plain`
    /// are exactly what a future text-level converter would pass. Which of them
    /// has a caller today is recorded in `a_profile_with_no_caller_is_named`.
    fn profiles() -> BTreeMap<&'static str, HandledDelimiters<'static>> {
        BTreeMap::from([
            ("plain", HandledDelimiters::PLAIN),
            ("markdown", HandledDelimiters::MARKDOWN),
            ("djot", HandledDelimiters::DJOT),
        ])
    }

    fn corpus() -> serde_json::Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/spec/tests/corpus-escape/cases.json");
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "escaper corpus not found at {}: {e}\n\
                 Did you initialize the submodule?\n  git submodule update --init",
                path.display()
            )
        });
        serde_json::from_str(&raw).expect("escaper corpus is JSON")
    }

    #[test]
    fn the_handled_sets_match_the_corpus_profiles() {
        // The sets are spelled in two places - here and in the corpus - and a
        // silent drift between them would make every case below pass while
        // measuring the wrong question.
        let corpus = corpus();
        let declared = corpus["profiles"]
            .as_object()
            .expect("profiles is an object");
        for (name, handled) in profiles() {
            let entry = declared
                .get(name)
                .unwrap_or_else(|| panic!("corpus declares no profile {name}"));
            assert_eq!(
                entry.get("braced").and_then(|v| v.as_str()).unwrap_or(""),
                handled.braced,
                "{name}: braced handled set"
            );
            assert_eq!(
                entry.get("bare").and_then(|v| v.as_str()).unwrap_or(""),
                handled.bare,
                "{name}: bare handled set"
            );
        }
    }

    #[test]
    fn a_case_is_read() {
        // Guards the sweep below against a glob that quietly matches nothing -
        // a checker reporting success having compared nothing is the state this
        // rule was already in.
        let corpus = corpus();
        let cases = corpus["cases"].as_array().expect("cases is an array");
        assert!(cases.len() >= 50, "found {} cases", cases.len());
    }

    #[test]
    fn every_case_matches_under_every_profile() {
        let corpus = corpus();
        let profiles = profiles();
        let mut checked = 0usize;
        let mut failures: Vec<String> = Vec::new();
        for case in corpus["cases"].as_array().expect("cases is an array") {
            let name = case["name"].as_str().expect("a case has a name");
            let input = case["input"].as_str().expect("a case has an input");
            for (profile, expected) in case["expected"]
                .as_object()
                .expect("a case has expectations")
            {
                let Some(handled) = profiles.get(profile.as_str()) else {
                    continue;
                };
                let expected = expected.as_str().expect("an expectation is a string");
                let got = escape_plain_carve_syntax(input, *handled);
                if got != expected {
                    failures.push(format!(
                        "{name} [{profile}]: {input:?} -> {got:?}, expected {expected:?}"
                    ));
                }
                checked += 1;
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {checked} escaper cases diverge:\n{}",
            failures.len(),
            failures.join("\n")
        );
        assert!(checked >= 150, "checked only {checked} case-profile pairs");
    }

    #[test]
    fn escaping_only_ever_inserts_backslashes() {
        // The corpus's own invariant, restated against THIS implementation: an
        // expectation is a fabrication if removing its backslashes does not give
        // the input back. Asserting it on the output rather than on the fixture
        // catches an escaper that rewrites text instead of freezing it.
        let corpus = corpus();
        for case in corpus["cases"].as_array().expect("cases is an array") {
            let input = case["input"].as_str().expect("a case has an input");
            for (profile, handled) in profiles() {
                let got = escape_plain_carve_syntax(input, handled);
                assert_eq!(
                    got.replace('\\', ""),
                    *input,
                    "{} [{profile}] rewrote its input",
                    case["name"]
                );
            }
        }
    }

    #[test]
    fn a_delimiter_with_a_space_against_it_is_where_the_opener_test_bites() {
        // `braced_run_opens` is the test that separates "does not open" from
        // "opens and is never closed", and the corpus does NOT reach it: its
        // only inner-space case is `a { ^x^ } b`, whose space sits between the
        // `{` and the delimiter, so the loop never matches the opener at all and
        // every answer this test could give passes. The case that reaches it has
        // the space AFTER the delimiter, and it is pinned here instead.
        assert_eq!(
            escape_plain_carve_syntax("a { ^x^ } b", HandledDelimiters::PLAIN),
            "a { ^x^ } b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a {^ x^} b", HandledDelimiters::PLAIN),
            "a {^ x^} b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a {^ x b", HandledDelimiters::PLAIN),
            "a {^ x b"
        );

        // STATED, NOT ASSERTED AS CORRECT. `a {^ x^} b` renders `a <sup> x</sup>
        // b` in Carve, so the second line above is a LEAK: text that is literal
        // in the calling language reaches Carve as markup. The same holds on the
        // Djot path for the delimiters Djot does not own - `a {, x,} b` is
        // literal Djot and renders `a <sub> x</sub> b` as Carve. Widening the
        // opener to accept a space against the delimiter fixes it, and is left
        // alone here on purpose: the shared corpus has no case for it, carve-php
        // passes the same corpus with the same boundary, and moving one engine
        // off an unpinned shape is how the four spellings drifted apart in the
        // first place. It wants a corpus case and all three engines, not a
        // unilateral change under a ticket about exposing the function.
    }

    #[test]
    fn the_profiles_remain_distinct() {
        // Djot and BBCode are text-level converters. Markdown and HTML build an
        // AST and let the canonical writer emit source, so they escape no text
        // and Markdown passes no handled set.
        // The handled set is what separates the profiles: `*` is Djot markup and
        // is left for the converter, and is literal text under PLAIN. Asserting
        // the two differ is what keeps the parameter load-bearing - a hardwired
        // set would make every profile give the same answer and every case above
        // would still pass.
        assert_eq!(
            escape_plain_carve_syntax("a *x* b", HandledDelimiters::DJOT),
            "a *x* b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a *x* b", HandledDelimiters::PLAIN),
            "a \\*x* b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a ~x~ b", HandledDelimiters::MARKDOWN),
            "a ~x~ b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a ~x~ b", HandledDelimiters::PLAIN),
            "a \\~x~ b"
        );
    }

    /// An at-sign that opens a Carve mention is escaped when it arrives as
    /// text. The sibling of the tag rule, ported from carve-php#1381.
    #[test]
    fn an_at_sign_in_source_text_is_not_a_mention() {
        for (input, want) in [
            ("hi @user ok", "hi \\@user ok"),
            ("@click toggles it", "\\@click toggles it"),
            ("use @keydown.window here", "use \\@keydown.window here"),
            ("see (@can) there", "see (\\@can) there"),
            ("the @-form", "the \\@-form"),
            ("@can and @click", "\\@can and \\@click"),
        ] {
            assert_eq!(
                escape_plain_carve_syntax(input, HandledDelimiters::DJOT),
                want,
                "input {input:?}"
            );
        }
    }

    /// BOUND: the escape mirrors the parser's opener, so an at-sign the parser
    /// never opens on gains no backslash.
    #[test]
    fn an_at_sign_that_opens_nothing_is_left_bare() {
        for input in [
            "mail me at foo@bar.de",
            "a@b",
            "name @ handle",
            "ping @, later",
            "ends with @",
        ] {
            assert_eq!(
                escape_plain_carve_syntax(input, HandledDelimiters::DJOT),
                input,
                "input {input:?}"
            );
        }
    }

    /// BOUND: an at-sign the source already escaped is not escaped twice.
    #[test]
    fn an_already_escaped_at_sign_is_left_alone() {
        assert_eq!(
            escape_plain_carve_syntax("hi \\@user ok", HandledDelimiters::DJOT),
            "hi \\@user ok"
        );
    }

    /// ONLY THE OPENING COLON. Escaping the opener is what makes the whole
    /// shortcode text - the closing colon then has a letter against it and
    /// opens nothing - so a second escape would be bytes PART 11 §4 asks the
    /// writer not to spend. The corpus case `a-symbol-shortcode` pins the
    /// one-escape form; this names WHICH colon under every profile.
    #[test]
    fn a_symbol_shortcode_is_frozen_at_its_opener_only() {
        for (input, expected) in [
            ("a :rocket: b", "a \\:rocket: b"),
            (":rocket:", "\\:rocket:"),
            // The reaction shortcodes: `+` and `-` open a name, `_` does not.
            ("a :+1: b", "a \\:+1: b"),
            ("a :-1: b", "a \\:-1: b"),
            // Two shortcodes against each other are two openers.
            (":a::b:", "\\:a:\\:b:"),
        ] {
            for (profile, handled) in profiles() {
                assert_eq!(
                    escape_plain_carve_syntax(input, handled),
                    expected,
                    "{input:?} under {profile}"
                );
            }
        }
    }

    /// BOUND: THE NEAR NEIGHBOUR THAT MUST NOT MOVE. `a : b : c` is the shape a
    /// rule over every colon would break, and the corpus pins it unchanged
    /// (`a-colon-that-closes-no-shortcode`). The parser opens no symbol at
    /// either colon - a space is not a name character - so neither is offered
    /// an escape here.
    #[test]
    fn a_colon_that_opens_no_shortcode_is_left_bare() {
        for input in [
            // The corpus's own bound.
            "a : b : c",
            // A colon with a letter against it is not an opener.
            "https://example.com",
            "note: see below",
            "12:30:45",
            // A name that never closes.
            "a :rocket b",
            // The fence and the definition marker, which are line-initial
            // constructs the converters already own.
            ":::",
            ": definition",
            // Nothing after the colon at all.
            "ends with :",
        ] {
            for (profile, handled) in profiles() {
                assert_eq!(
                    escape_plain_carve_syntax(input, handled),
                    input,
                    "{input:?} under {profile}"
                );
            }
        }
    }

    /// BOUND: `_` CANNOT OPEN A NAME, because `:_x_:` would steal from
    /// underline - `parse_symbol` excludes it from the first name character's
    /// class and this mirrors that. Asserted on the colon alone: the `_x_` in
    /// it is a bare underline run, which the profiles that do not spell `_`
    /// freeze for their own unrelated reason.
    #[test]
    fn an_underscore_does_not_open_a_symbol_name() {
        for (profile, handled) in profiles() {
            let got = escape_plain_carve_syntax("a :_x_: b", handled);
            assert!(!got.contains("\\:"), "{profile} escaped a colon in {got:?}");
        }
    }

    /// BOUND: a colon the source already escaped is not escaped twice.
    #[test]
    fn an_already_escaped_symbol_opener_is_left_alone() {
        assert_eq!(
            escape_plain_carve_syntax("a \\:rocket: b", HandledDelimiters::DJOT),
            "a \\:rocket: b"
        );
    }
}

#[cfg(test)]
mod attributed_strong_tests {
    #[test]
    fn attributes_do_not_close_strong_delimiters() {
        for (source, expected) in [
            (
                "a *word{#id key=\"*\"}*",
                "<p>a <strong><span id=\"id\" key=\"*\">word</span></strong></p>",
            ),
            (
                "*more words{#id key=\"*\"} here*",
                "<p><strong>more <span id=\"id\" key=\"*\">words</span> here</strong></p>",
            ),
            (
                "`*word{#id key=\"*\"}*`",
                "<p><code>*word{#id key=\"*\"}*</code></p>",
            ),
        ] {
            assert_eq!(crate::to_html(&super::djot_to_carve(source)), expected);
        }
    }
}

#[cfg(test)]
mod attribute_list_tests {
    use super::djot_to_carve;

    #[test]
    fn valid_attribute_lists_keep_their_delimiters() {
        for source in [
            "{#id .class}\nA paragraph\n",
            "{#id .class\n  style=\"color:red\"}\nA paragraph\n",
            "{#id}\n> Block quote\n",
            "{#id}\n# Heading\n",
            "[nested [span]{.blue}]{#ident}\n",
            "[span]{title=\"_*#literal*\"}\n",
        ] {
            assert_eq!(djot_to_carve(source), source, "{source}");
        }
    }

    #[test]
    fn rejected_attributes_do_not_hide_paragraph_boundaries_or_later_spans() {
        let source = "_a [x]{.a\n\n.b} b_";
        assert!(!djot_to_carve(source).contains("/a"));
        assert!(!crate::to_html(&djot_to_carve("para\n{#id}\nnext")).contains("id=\"id\""));
        assert!(djot_to_carve("a { \"q\n[x]{title=\"_a_\"}").contains("[x]{title=\"_a_\"}"));
        assert!(djot_to_carve("[x]{#a\n.b}").contains("\\#a"));
        let invalid = "[x]{k=a*b*}";
        assert_eq!(super::mask_djot_attributes(invalid), invalid);
    }

    #[test]
    fn attributes_in_code_and_invalid_lists_remain_literal() {
        for source in ["`[x]{#id}`", "```\n[x]{#id}\n```"] {
            let written = djot_to_carve(source);
            assert_eq!(crate::to_html(source), crate::to_html(&written));
        }
        assert_eq!(djot_to_carve("[span]{#a<b}"), "[span]\\{\\#a<b}");
        assert_eq!(djot_to_carve("a #tag"), "a \\#tag");
        assert_eq!(crate::to_html(&djot_to_carve("\\{#id}")), "<p>{#id}</p>");
    }
}

#[cfg(test)]
mod heading_continuation_tests {
    use super::djot_to_carve;

    #[test]
    fn headings_fold_matching_markers_and_lazy_lines() {
        for (source, expected) in [
            ("# Heading\n# continued\n", "# Heading continued\n"),
            ("# Heading\nlazy\n", "# Heading lazy\n"),
            (
                "# Heading\nlazy\n# more\nlazy\n\ntext\n",
                "# Heading lazy more lazy\n\ntext\n",
            ),
            ("## A\n## B\nC", "## A B C"),
            ("# A\n  # B\n  C", "# A B C"),
            ("# A\n    B", "# A B"),
        ] {
            assert_eq!(djot_to_carve(source), expected);
        }
    }

    #[test]
    fn headings_stop_at_blocks_and_leave_code_alone() {
        for next in [
            "",
            "## B",
            "#",
            "- item",
            "1. item",
            "> quote",
            "```",
            "~~~",
            "[r]: /url",
            "::: div",
            "  - item",
            "***",
            "---",
            "* * *",
            "^ x",
            "%%%",
        ] {
            let source = format!("# A\n{next}\n");
            assert_eq!(
                djot_to_carve(&source),
                format!("# A\n{}", djot_to_carve(&format!("{next}\n")))
            );
        }
        assert_eq!(djot_to_carve("# A\n{.class}\n"), "# A\n");
        for source in [
            "```\n# A\n# B\n```\n",
            "`x\n# A\n# B\ny`\n",
            "para\n# A\nB\n",
            "# A\\\nB\n",
        ] {
            assert_eq!(djot_to_carve(source), source);
        }
    }
}
