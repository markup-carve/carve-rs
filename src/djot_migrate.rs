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

macro_rules! cached_regex {
    ($pattern:literal) => {{
        static COMPILED: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        Ok::<&'static regex::Regex, regex::Error>(
            COMPILED.get_or_init(|| regex::Regex::new($pattern).unwrap()),
        )
    }};
}

#[path = "djot_emphasis.rs"]
mod emphasis;

use std::collections::HashSet;

use crate::ast::{BlockNode, FigureTarget};

/// Convert Djot source to Carve source.
pub fn djot_to_carve(djot: &str) -> String {
    let normalized = djot.replace("\r\n", "\n").replace('\r', "\n");
    let (frontmatter, separator, body) = split_frontmatter(&normalized);
    let attributes = normalize_djot_attribute_lines(body);
    let fence_closers = normalize_djot_fences(&attributes);
    let references = fold_djot_references(&fence_closers);
    let footnotes = normalize_djot_footnotes(&references);
    let links = normalize_djot_links(&footnotes);
    let autolinks = normalize_djot_autolinks(&links);
    let table_pipes = normalize_djot_table_pipes(&autolinks);
    let folded = fold_heading_continuations(&table_pipes);
    let collapsed = cached_regex!(r"(!?\[([^\[\]\n]*)\])\[\]").unwrap();
    let mut collapsed_mask = mask_code_and_destinations(&folded).into_bytes();
    let mut at = 0;
    while at < folded.len() {
        if collapsed_mask[at] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(&folded, at) {
                for byte in &mut collapsed_mask[at..end] {
                    if *byte != b'\n' {
                        *byte = b' ';
                    }
                }
                at = end;
                continue;
            }
        }
        at += 1;
    }
    let collapsed_mask =
        String::from_utf8(collapsed_mask).expect("attribute masks preserve UTF-8 boundaries");
    let autolink = cached_regex!(r"<[^<>\s]+>").unwrap();
    let collapsed_mask = autolink.replace_all(&collapsed_mask, |caps: &regex::Captures<'_>| {
        if caps[0].contains(':') || caps[0].contains('@') {
            " ".repeat(caps[0].len())
        } else {
            caps[0].to_string()
        }
    });
    let definitions_regex =
        cached_regex!(r"(?m)^[ \t]*(?:> ?)*(?:(?:[-*+]|[0-9]+[.)])[ \t]+)?\[([^\[\]\n]*)\]:[ \t]")
            .unwrap();
    let definition_boundary =
        cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\{[.#A-Za-z]|\[[^\]]*\]:)").unwrap();
    let definitions: std::collections::HashSet<String> = definitions_regex
        .captures_iter(&folded)
        .filter(|caps| {
            let start = caps.get(0).unwrap().start();
            let previous = folded[..start]
                .split('\n')
                .rev()
                .nth(1)
                .unwrap_or("")
                .trim_start_matches([' ', '\t', '>'])
                .trim();
            collapsed_mask.as_bytes()[start + caps[0].find('[').unwrap()] == b'['
                && (previous.is_empty() || definition_boundary.is_match(previous))
        })
        .map(|caps| caps[1].to_string())
        .collect();
    let folded = collapsed.replace_all(&folded, |caps: &regex::Captures<'_>| {
        if collapsed_mask.as_bytes()[caps.get(0).unwrap().start()] == caps[0].as_bytes()[0]
            && definitions.contains(&caps[2])
        {
            format!("{}[{}]", &caps[1], &caps[2])
        } else {
            caps[0].to_string()
        }
    });
    let mut alt_prefix = "\0DJOTALT\0".to_string();
    while folded.contains(&alt_prefix) {
        alt_prefix.push('\0');
    }
    let mut alts = Vec::new();
    let mut alt_cache = std::collections::HashMap::<String, String>::new();
    let image = cached_regex!(r"!\[([^\[\]\n]*)\]([\[(])").unwrap();
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
            if let Some(alt) = alt_cache.get(label) {
                alts.push(alt.clone());
                return format!("![{alt_prefix}{}\0]{}", alts.len() - 1, &caps[2]);
            }
            let alt = crate::to_plain_text_with_options(
                &djot_to_carve(&format!(
                    "DJOTALT {} DJOTEND",
                    strip_image_alt_attributes(label)
                )),
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
            .to_string();
            alt_cache.insert(label.to_string(), alt.clone());
            alts.push(alt);
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
    let converted = if spans.is_empty() {
        converted
    } else {
        let restore =
            regex::Regex::new(&format!(r"{}([0-9]+)\x00", regex::escape(&prefix))).unwrap();
        restore
            .replace_all(&converted, |caps: &regex::Captures<'_>| {
                spans[caps[1].parse::<usize>().unwrap()].clone()
            })
            .into_owned()
    };

    let converted = if alts.is_empty() {
        converted
    } else {
        let alt_restore =
            regex::Regex::new(&format!(r"{}([0-9]+)\x00", regex::escape(&alt_prefix))).unwrap();
        alt_restore
            .replace_all(&converted, |caps: &regex::Captures<'_>| {
                alts[caps[1].parse::<usize>().unwrap()].clone()
            })
            .into_owned()
    };
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

fn strip_image_alt_attributes(label: &str) -> String {
    let mut mask = mask_code_and_destinations(label).into_bytes();
    for value in cached_regex!(r"<[^<>\s]+>").unwrap().find_iter(label) {
        if value.as_str().contains(':') || value.as_str().contains('@') {
            mask[value.range()].fill(b' ');
        }
    }
    let bytes = label.as_bytes();
    let mut out = String::new();
    let mut cursor = 0;
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'{' && mask[at] == b'{' && !is_escaped(bytes, at) {
            if let Some((end, _)) = read_djot_word_attributes(label, at) {
                out.push_str(&label[cursor..at]);
                cursor = end;
                at = end;
                continue;
            }
        }
        at += 1;
    }
    out.push_str(&label[cursor..]);
    out
}

fn djot_attribute_context(source: &str, start: usize) -> (usize, Option<usize>, usize) {
    let mut prefix = &source[source[..start].rfind('\n').map_or(0, |p| p + 1)..start];
    let quote = cached_regex!(r"^[ \t]*>(?:[ \t]|$)").unwrap();
    let mut depth = 0;
    while let Some(m) = quote.find(prefix) {
        prefix = &prefix[m.len()..];
        depth += 1;
    }
    let marker =
        cached_regex!(r"^[ \t]*(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\))[ \t]+").unwrap();
    let block =
        cached_regex!(r"^(?:[ \t]*(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\))[ \t]+)?[ \t]*$")
            .unwrap();
    (
        depth,
        block.is_match(prefix).then_some(prefix.len()),
        marker.find(prefix).map_or(0, |m| m.len()),
    )
}
fn djot_attribute_line(mut line: &str, depth: usize) -> Option<&str> {
    let quote = cached_regex!(r"^[ \t]*>(?:[ \t]|$)").unwrap();
    for _ in 0..depth {
        line = &line[quote.find(line)?.len()..];
    }
    Some(line)
}

fn read_djot_word_attributes(source: &str, start: usize) -> Option<(usize, String)> {
    let bytes = source.as_bytes();
    let quote_prefix = cached_regex!(r"^[ \t]*>(?:[ \t]|$)").unwrap();
    let mut parts = Vec::new();
    let mut i = start + 1;
    let mut context = None;
    while i < bytes.len() {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
            if bytes[i] == b'\n' && blank_line_follows(bytes, i) {
                return None;
            }
            if bytes[i] == b'\n' {
                let (depth, _, _) =
                    *context.get_or_insert_with(|| djot_attribute_context(source, start));
                i += 1;
                for _ in 0..depth {
                    let Some(quote) = quote_prefix.find(&source[i..]) else {
                        break;
                    };
                    i += quote.end();
                }
            } else {
                i += 1;
            }
        }
        if bytes.get(i) == Some(&b'}') && !parts.is_empty() && source[start..=i].contains('\n') {
            let (depth, block_indent, minimum) = djot_attribute_context(source, start);
            for raw in source[start..=i].split('\n').skip(1) {
                let line = if block_indent.is_some() {
                    djot_attribute_line(raw, depth)?
                } else {
                    djot_attribute_line(raw, depth).unwrap_or(raw)
                };
                let indent = line
                    .bytes()
                    .take_while(|b| matches!(b, b' ' | b'\t'))
                    .count();
                if block_indent.is_some_and(|owner| indent < minimum || indent <= owner) {
                    return None;
                }
            }
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
                    if bytes[i] == b'\n' && blank_line_follows(bytes, i) {
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
                let value = &source[from..i];
                let folded = if value.contains('\n') {
                    let (depth, _, _) = djot_attribute_context(source, start);
                    let mut lines = value.split('\n');
                    let mut folded = lines.next().unwrap().to_owned();
                    for raw in lines {
                        folded.push(' ');
                        folded.push_str(
                            djot_attribute_line(raw, depth)
                                .unwrap_or(raw)
                                .trim_start_matches([' ', '\t']),
                        );
                    }
                    folded
                } else {
                    value.to_owned()
                };
                parts.push(format!("{key}{folded}"));
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
        r"^(?:[#>|{]|[-*+][ \t]|[0-9]+[.)][ \t]|:[ \t]|:{2,}|\([0-9a-zA-Z]+\)[ \t]|[`~]{3,}|\^[ \t]|%{3,}|\[[^\]\n]*\]:|(?:\*[ \t]*){3,}$|(?:-[ \t]*){3,}$)").unwrap());
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

fn convert_djot_block_markers(source: &str) -> String {
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let masked_source = mask_code_and_destinations(source);
    let masked: Vec<&str> = masked_source.split('\n').collect();
    let mut nested_lines = Vec::with_capacity(masked.len());
    let mut ancestors: Vec<(usize, bool)> = Vec::new();
    for line in &masked {
        if line.trim().is_empty() {
            nested_lines.push(false);
            continue;
        }
        let (columns, _) = leading_indent(line);
        while ancestors.last().is_some_and(|top| top.0 >= columns) {
            ancestors.pop();
        }
        nested_lines.push(ancestors.last().is_some_and(|top| top.1));
        ancestors.push((columns, list_marker(line)));
    }
    let mut containers: Vec<(usize, bool)> = Vec::new();
    let host_prefix = cached_regex!(r"^(?:[ \t]*> ?|[ \t]*(?:(?:[-*+]|(?:[0-9]+|[ivxlcdm]+|[IVXLCDM]+|[a-zA-Z])[.)]|\([0-9A-Za-z]+\)) +(?:\[[ xX]\] +)?|: |\[\^[^\]\r\n]+\]: +))").unwrap();
    for index in 0..lines.len() {
        let prefix = quote_prefix_len(masked[index]);
        let quote = &masked[index][..prefix];
        let rest = &masked[index][prefix..];
        let (_, indent_bytes) = leading_indent(rest);
        let content = &rest[indent_bytes..];
        if content.trim().is_empty() {
            continue;
        }
        let mut container_view = lines[index].as_str();
        while let Some(host) = host_prefix.find(container_view) {
            container_view = &container_view[host.end()..];
        }
        container_view = container_view.trim_start_matches([' ', '\t']);
        let container_offset = lines[index].len() - container_view.len();
        let authored = &lines[index][container_offset..];
        let unmasked = masked[index][container_offset..].starts_with(":::");
        let width = if unmasked {
            crate::parse::lint_container_fence_width(authored)
        } else {
            None
        };
        let close = width.is_some()
            && authored
                .trim_end_matches([' ', '\t'])
                .bytes()
                .all(|b| b == b':')
            && containers.last().is_some_and(|top| Some(top.0) == width);
        let invalid = if close {
            containers.pop().unwrap().1
        } else {
            let invalid = unmasked && crate::parse::lint_invalid_container_metadata(authored);
            if let Some(width) = width {
                containers.push((width, invalid));
            }
            invalid
        };
        if invalid {
            let at = container_offset;
            lines[index].insert(at, '\\');
        }
        if let Some(close) = content.strip_prefix('(').and_then(|value| value.find(')')) {
            let token = &content[1..close + 1];
            let tail = &content[close + 2..];
            if !token.is_empty()
                && token.chars().all(|ch| ch.is_ascii_alphanumeric())
                && tail.starts_with([' ', '\t'])
                && !tail.trim().is_empty()
            {
                let nested = quote.is_empty() && nested_lines[index];
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
            let nested = quote.is_empty() && nested_lines[index];
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
        let attached_span = i > 0
            && matches!(bytes[i - 1], b']' | b')')
            && !bytes[i..end.min(bytes.len())].contains(&b'\n');
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
        if self.values.is_empty() {
            return source.to_string();
        }
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
    let uri = cached_regex!(r"<[A-Za-z][A-Za-z0-9+.-]*:[^<>\s]*>").unwrap();
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
    let block_end = cached_regex!(
        r"^(?:`{3,}|~{3,}|:{3,}|#{1,6} |[-*+] |[0-9]+[.)] |> |:{1,2} |(?:\*[ \t]*){3,}|(?:-[ \t]*){3,}|\|.*\||\[[^\]]+\]:)")
    .unwrap();
    let quote_prefix = cached_regex!(r"^(?:[ \t]*>[ \t]*)*[ \t]*").unwrap();
    let marker = cached_regex!(
        r"^(?:[ \t]*>[ \t]*)*[ \t]*(?:[-*+]|[0-9]+[.)]|#{1,6}|:{1,2}|\[\^[^\]]+\]:)[ \t]+(?:\[[ xX-]\][ \t]+)?$")
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
        let stripped_line = pattern.replace_all(line, "");
        let marker_only = marker.is_match(&stripped_line);
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
            if marker_only {
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
    let autolink = cached_regex!(r"<[^<>\s]+>").unwrap();
    let scheme = cached_regex!(r"[^:]@|[A-Za-z]:").unwrap();
    for value in autolink.find_iter(source) {
        if scheme.is_match(value.as_str()) {
            mask[value.range()].fill(b' ');
        }
    }
    for label in cached_regex!(r"\[\^[^\]\n]*\]").unwrap().find_iter(source) {
        mask[label.range()].fill(b' ');
    }
    let refs = cached_regex!(
        r"(?m)^[ \t]*(?:>[ \t]*)*(?:(?:[-*+]|[0-9]+[.)])[ \t]+)?\[[^\^\]\n][^\]\n]*\]:[^\n]*"
    )
    .unwrap();
    let reference_boundary =
        cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\{[.#A-Za-z]|\[[^\]]*\]:)").unwrap();
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
    let images = cached_regex!(r"!\[[^\[\]\n]*\][\[(]").unwrap();
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
    let mut taken: std::collections::HashMap<char, std::collections::BTreeMap<usize, usize>> =
        std::collections::HashMap::new();

    for rule in RULES {
        for (start, end, inner_start, inner_end) in find_pairs(&masked, rule) {
            // One delimiter run belongs to one rule. A `~~x~~` claimed by the
            // strikethrough rule must not be re-read as two subscripts.
            let ranges = taken.entry(rule.family).or_default();
            if ranges
                .range(..=start)
                .next_back()
                .is_some_and(|(_, &previous_end)| previous_end > start)
                || ranges
                    .range(start..)
                    .next()
                    .is_some_and(|(&next_start, _)| next_start < end)
            {
                continue;
            }
            ranges.insert(start, end);
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
            Ok(inner_end) => {
                found.push((i, inner_end + closer_width, inner_start, inner_end));
                i = inner_end + closer_width;
            }
            Err(paragraph_end) => i = paragraph_end,
        }
    }

    found
}

fn find_closer(masked: &str, from: usize, rule: &Rule) -> Result<usize, usize> {
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
            return Err(j + 1);
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

            return Ok(j);
        }

        j += 1;
    }

    Err(bytes.len())
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
    mask_djot_inline(source, true)
}

fn mask_djot_inline(source: &str, inline_forms: bool) -> String {
    mask_djot_forms(source, inline_forms, true, None, &[])
}

type FenceLineCallback<'a> = &'a mut dyn FnMut(usize, &str);

fn mask_djot_fences(
    source: &str,
    mut on_fence_line: Option<FenceLineCallback<'_>>,
    row_boundaries: &[bool],
    strict: bool,
) -> String {
    let mut mask = source.as_bytes().to_vec();
    let opener = cached_regex!(r"^([ \t]*)(?:(\[\^[^\]\n]+\]:[ \t]*|(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)|:)[ \t]+))?(`{3,}|~{3,})[ \t]*=?[A-Za-z0-9_+#.-]*[ \t]*$").unwrap();
    let boundary =
        cached_regex!(r"^[ \t]*(?:[-*+] |[0-9]+[.)] |:{1,2} |#{1,6} |\{[.#A-Za-z])").unwrap();
    let mut fence: Option<(u8, usize, usize, Option<usize>, usize)> = None;
    let mut previous_block = true;
    let mut fence_target = String::new();
    let mut fence_dedent = 0;
    let mut fence_normalize = false;
    let mut normalize_boundary = true;
    let normalize_boundary_pattern =
        cached_regex!(r"^[ \t]*(?:#{1,6} |:{3,}|\{[.#A-Za-z])").unwrap();
    let mut offset = 0;
    let marker_pattern = cached_regex!(r"^[ \t]*(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)|:)[ \t]+\S").unwrap();
    let thematic_pattern = cached_regex!(r"^(?:([*-])[ \t]*){3,}$").unwrap();
    let native_marker = cached_regex!(r"\(([0-9A-Za-z]+)\)([ \t]+)$").unwrap();
    let marker_prefix = cached_regex!(r"^[ \t]*(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)|:)[ \t]+").unwrap();
    let footnote_prefix = cached_regex!(r"^([ \t]*(?:(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)|:)[ \t]+)*)\[\^[^\]\n]+\]:").unwrap();
    let empty_footnote = cached_regex!(r"^[ \t]*\[\^[^\]\n]+\]:[ \t]*$").unwrap();
    let mut previous_depth = 0;
    let block_definition = cached_regex!(r"^[ \t]*\[[^\^\]][^\]]*\]:").unwrap();
    let mut ancestors: Vec<Vec<(usize, usize, usize)>> = Vec::new();
    for (line_index, raw) in source.split_inclusive('\n').enumerate() {
        let line = raw.trim_end_matches('\n');
        let (depth, content) = quoted(line);
        let mut can_normalize = normalize_boundary
            || row_boundaries.get(line_index.wrapping_sub(1)) == Some(&true)
            || depth < previous_depth;
        previous_depth = depth;
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
        let mut nested = false;
        let mut owner_column = 0;
        let mut owner_indent = 0;
        ancestors.truncate(depth + 1);
        if fence.is_none() && !content.trim().is_empty() {
            ancestors.resize_with(depth + 1, Vec::new);
            let mut view = line;
            for (level, stack) in ancestors.iter_mut().enumerate() {
                let columns = view.len() - view.trim_start_matches([' ', '\t']).len();
                while stack.last().is_some_and(|(prior, _, _)| *prior >= columns) {
                    if level == depth
                        && stack.last().unwrap().1 > 0
                        && columns <= stack.last().unwrap().2
                    {
                        can_normalize = true;
                    }
                    stack.pop();
                }
                if level == depth {
                    owner_column = stack.last().map_or(0, |top| top.1);
                    owner_indent = stack.last().map_or(0, |top| top.2);
                    nested = owner_column > 0;
                }
                let marker =
                    !thematic_pattern.is_match(view.trim()) && marker_pattern.is_match(view);
                let prefix = marker_prefix.find(view).map_or("", |value| value.as_str());
                let note = footnote_prefix.captures(view);
                let column = if let Some(note) = &note {
                    native_marker.replace_all(&note[1], "$1.$2").len() + 2
                } else if marker {
                    native_marker.replace(prefix, "$1.$2").len()
                } else {
                    stack.last().map_or(0, |top| top.1)
                };
                let owning_indent = if let Some(note) = &note {
                    note[1].len()
                } else if marker {
                    columns
                } else {
                    stack.last().map_or(0, |top| top.2)
                };
                if note.is_some() && marker {
                    stack.push((
                        columns,
                        native_marker.replace(prefix, "$1.$2").len(),
                        columns,
                    ));
                }
                stack.push((
                    note.as_ref().map_or(columns, |note| note[1].len()),
                    column,
                    owning_indent,
                ));
                if level < depth {
                    let after = view
                        .trim_start_matches([' ', '\t'])
                        .strip_prefix('>')
                        .unwrap();
                    view = after.strip_prefix(' ').unwrap_or(after);
                }
            }
        }
        if let Some((ch, run, _, _, owner_depth)) = fence {
            blank_out(&mut mask, offset, offset + line.len());
            let candidate = &content[indent..];
            let length = candidate.bytes().take_while(|byte| *byte == ch).count();
            if depth == owner_depth
                && length >= run
                && candidate[length..].trim_matches([' ', '\t']).is_empty()
            {
                if let Some(callback) = on_fence_line.as_mut().filter(|_| fence_normalize) {
                    callback(
                        line_index,
                        &format!("{}{}{}", &line[..prefix_len], fence_target, candidate),
                    );
                }
                normalize_boundary = fence_normalize;
                fence = None;
                previous_block = true;
            } else if fence_normalize && depth == owner_depth {
                if let Some(callback) = on_fence_line.as_mut() {
                    callback(
                        line_index,
                        &format!(
                            "{}{}{}",
                            &line[..prefix_len],
                            fence_target,
                            &content[fence_dedent.min(indent)..]
                        ),
                    );
                }
            }
        } else if let Some(open) = opener.captures(content) {
            let marker = open.get(2).map_or("", |value| value.as_str());
            if (!marker.starts_with(':') || previous_block)
                && (!strict || can_normalize || !marker.is_empty())
            {
                let container = if !marker.is_empty() {
                    Some(open[1].len() + 1)
                } else if nested {
                    Some(owner_indent + 1)
                } else {
                    None
                };
                let target_column = if marker.starts_with("[^") {
                    open[1].len() + 2
                } else if !marker.is_empty() {
                    open[1].len() + native_marker.replace(marker, "$1.$2").len()
                } else if nested {
                    owner_column
                } else {
                    0
                };
                fence_normalize = can_normalize || !marker.is_empty();
                fence_dedent = open[1].len() + marker.len();
                fence_target = " ".repeat(target_column);
                let native_prefix = native_marker.replace(marker, "$1.$2");
                if fence_normalize && (marker.is_empty() || native_prefix != marker) {
                    if let Some(callback) = on_fence_line.as_mut() {
                        callback(
                            line_index,
                            &format!(
                                "{}{}{}",
                                &line[..prefix_len],
                                if marker.is_empty() {
                                    fence_target.clone()
                                } else {
                                    format!("{}{}", &open[1], native_prefix)
                                },
                                &content[fence_dedent..]
                            ),
                        );
                    }
                }
                let start = open.get(3).unwrap().start() + prefix_len;
                fence = Some((open[3].as_bytes()[0], open[3].len(), 0, container, depth));
                blank_out(&mut mask, offset + start, offset + line.len());
            }
        } else {
            normalize_boundary = thematic_pattern.is_match(content.trim())
                || block_definition.is_match(content)
                || content.trim().is_empty()
                || empty_footnote.is_match(content)
                || normalize_boundary_pattern.is_match(content);
            previous_block = content.trim().is_empty() || boundary.is_match(content);
        }
        offset += raw.len();
    }

    String::from_utf8(mask).expect("fence masks preserve UTF-8")
}

fn mask_djot_forms(
    source: &str,
    inline_forms: bool,
    footnotes: bool,
    on_fence_line: Option<FenceLineCallback<'_>>,
    row_boundaries: &[bool],
) -> String {
    let bytes = source.as_bytes();
    let footnote_ends = if inline_forms && footnotes && source.contains("[^") {
        let mut ends = vec![usize::MAX; bytes.len()];
        let mut close = usize::MAX;
        for i in (0..bytes.len()).rev() {
            if bytes[i] == b'\n' {
                close = usize::MAX;
            } else if bytes[i] == b']' {
                close = i;
            }
            ends[i] = close;
        }
        Some(ends)
    } else {
        None
    };

    let mut mask = mask_djot_fences(source, on_fence_line, row_boundaries, false).into_bytes();

    let paragraph_ends: Vec<usize> = cached_regex!(r"\n[ \t]*(?:>[ \t]*)*\n")
        .unwrap()
        .find_iter(source)
        .map(|value| value.start())
        .collect();
    let autolinks: std::collections::HashMap<usize, usize> = cached_regex!(r"<[^<>\s]+>")
        .unwrap()
        .find_iter(source)
        .filter(|value| {
            cached_regex!(r"[^:]@|[A-Za-z]:")
                .unwrap()
                .is_match(value.as_str())
        })
        .map(|value| (value.start(), value.end()))
        .collect();
    let mut destination_ends = std::collections::HashMap::new();
    if inline_forms && source.contains("](") {
        let mut close = None;
        for at in (0..bytes.len()).rev() {
            if bytes[at] == b')' {
                close = Some(at + 1);
            }
            if bytes[at..].starts_with(b"](") {
                if let Some(end) = close {
                    destination_ends.insert(at, end);
                }
            }
        }
    }
    let mut paragraph_index = 0;
    let mut i = 0;
    while i < bytes.len() {
        if mask[i] != bytes[i] {
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' && bytes.get(i + 1).is_some_and(u8::is_ascii_punctuation) {
            i += 2;
            continue;
        }
        if let Some(end) = autolinks.get(&i) {
            i = *end;
            continue;
        }
        // An inline code span, delimited by a matching backtick run.
        if bytes[i] == b'`' {
            let run = bytes[i..].iter().take_while(|b| **b == b'`').count();
            while paragraph_ends
                .get(paragraph_index)
                .is_some_and(|end| *end <= i)
            {
                paragraph_index += 1;
            }
            let paragraph_end = paragraph_ends
                .get(paragraph_index)
                .copied()
                .unwrap_or(bytes.len());
            if let Some(close) = find_backtick_close(&bytes[..paragraph_end], i + run, run) {
                blank_out(&mut mask, i, close + run);
                i = close + run;
                continue;
            } else {
                blank_out(&mut mask, i, paragraph_end);
                i = paragraph_end;
                continue;
            }
        }

        // A link or image destination: `](...)`.
        if inline_forms && bytes[i] == b']' && bytes.get(i + 1) == Some(&b'(') {
            if let Some(end) = destination_ends.get(&i).copied() {
                blank_out(&mut mask, i + 1, end);
                i = end;
                continue;
            }
        }

        // A footnote reference is one opaque token. In particular, the two
        // carets in adjacent references must never pair as superscript.
        if inline_forms && footnotes && bytes[i..].starts_with(b"[^") {
            let close = footnote_ends
                .as_ref()
                .and_then(|ends| ends.get(i + 2))
                .copied()
                .unwrap_or(usize::MAX);
            if close != usize::MAX {
                let end = close + 1;
                blank_out(&mut mask, i, end);
                i = end;
                continue;
            }
        }

        i += 1;
    }

    let metadata = cached_regex!(r"(?m)^(?:[ \t]*>)*[ \t]*(?:(?:[-*+]|[0-9]+[.)])[ \t]+)?:{3,}[ \t]+([A-Za-z_][A-Za-z0-9_.-]*)").unwrap();
    let item_metadata = cached_regex!(r"(?:[-*+]|[0-9]+[.)])[ \t]+:{3,}").unwrap();
    for caps in metadata.captures_iter(source) {
        let value = caps.get(0).unwrap();
        let previous = source[..value.start()]
            .split('\n')
            .rev()
            .nth(1)
            .unwrap_or("");
        if previous.trim().is_empty() || item_metadata.is_match(value.as_str()) {
            mask[caps.get(1).unwrap().range()].fill(b' ');
        }
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
            "{#id}\n> Block quote\n",
            "{#id}\n# Heading\n",
            "[nested [span]{.blue}]{#ident}\n",
            "[span]{title=\"_*#literal*\"}\n",
        ] {
            assert_eq!(djot_to_carve(source), source, "{source}");
        }
    }

    #[test]
    fn multiline_attribute_lists_are_canonicalized() {
        assert_eq!(
            djot_to_carve("{#id .class\n  style=\"color:red\"}\nA paragraph\n"),
            "{#id .class style=\"color:red\"}\nA paragraph\n"
        );
    }

    #[test]
    fn rejected_attributes_do_not_hide_paragraph_boundaries_or_later_spans() {
        let source = "_a [x]{.a\n\n.b} b_";
        assert!(!djot_to_carve(source).contains("/a"));
        assert!(!crate::to_html(&djot_to_carve("para\n{#id}\nnext")).contains("id=\"id\""));
        assert!(djot_to_carve("a { \"q\n[x]{title=\"_a_\"}").contains("[x]{title=\"_a_\"}"));
        assert_eq!(
            crate::to_html(&djot_to_carve("[x]{#a\n.b}")),
            "<p><span id=\"a\" class=\"b\">x</span></p>"
        );
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

fn normalize_djot_links(source: &str) -> String {
    use std::collections::HashMap;
    if !source.contains("](") {
        return source.to_owned();
    }
    let mask = mask_djot_inline(source, false);
    let rows = djot_table_rows(source, &mask);
    let bytes = source.as_bytes();
    let angles: HashMap<_, _> = cached_regex!(r"<[^<>\s]+>")
        .unwrap()
        .find_iter(source)
        .filter(|m| {
            cached_regex!(r"[^:]@|[A-Za-z]:")
                .unwrap()
                .is_match(m.as_str())
        })
        .map(|m| (m.start(), m.end()))
        .collect();
    #[derive(Clone)]
    struct Link {
        at: usize,
        depth: usize,
        label_end: usize,
        target: Option<usize>,
        parens: usize,
    }
    let quote_depths: Vec<_> = source
        .split('\n')
        .map(|line| {
            let prefix = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*")
                .unwrap()
                .find(line)
                .unwrap()
                .as_str();
            prefix.bytes().filter(|&b| b == b'>').count()
        })
        .collect();
    let mut stack: Vec<Link> = Vec::new();
    let mut pending_notes = HashSet::new();
    let mut literal_notes = HashMap::new();
    let mut edits = HashMap::<usize, (usize, String)>::new();
    let mut line = 0;
    let mut destination_owner: Option<usize> = None;
    let mut i = 0;
    let blank = cached_regex!(r"^\n[ \t]*(?:>[ \t]*)*\n").unwrap();
    let folds = cached_regex!(r"\n([ \t]*[^\n]*)").unwrap();
    let note_newline = cached_regex!(r"\n[ \t]*").unwrap();
    let escaped_char = cached_regex!(r"\\(?:\r?\n|[^\r\n])").unwrap();
    let escaped_pipe = cached_regex!(r"\\+\|").unwrap();
    let alt_end = cached_regex!(r" DJOTEND\n?$").unwrap();
    while i < source.len() {
        if bytes[i] == b'\n' {
            line += 1;
            if blank.is_match(&source[i..]) {
                stack.clear();
                pending_notes.clear();
                destination_owner = None;
                i += 1;
                continue;
            }
        }
        if mask.as_bytes()[i] == b' ' {
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if let Some(&end) = angles.get(&i) {
            i = end;
            continue;
        }
        if bytes[i] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(source, i) {
                i = end;
                continue;
            }
        }
        if rows.get(line) == Some(&true)
            && bytes[i] == b'|'
            && i.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\')
        {
            stack.clear();
            pending_notes.clear();
            destination_owner = None;
            i += 1;
            continue;
        }
        if bytes[i] == b'[' {
            if stack.len() >= 200 {
                return source.to_owned();
            }
            if bytes.get(i + 1) == Some(&b'^') {
                pending_notes.insert(i);
            }
            stack.push(Link {
                at: i,
                depth: quote_depths[line],
                label_end: 0,
                target: None,
                parens: 0,
            });
            i += 1;
            continue;
        }
        let Some(tip) = stack.len().checked_sub(1) else {
            i += 1;
            continue;
        };
        if bytes[i] == b']' {
            if bytes.get(stack[tip].at + 1) == Some(&b'^') {
                literal_notes.insert(stack[tip].at, i + 1);
                pending_notes.remove(&stack[tip].at);
                stack.pop();
                i += 1;
                continue;
            }
            if bytes.get(i + 1) == Some(&b'(') {
                if let Some(owner) = destination_owner {
                    if owner != tip {
                        let at = stack[owner].at;
                        edits.insert(at, (at + 1, "\\[".to_owned()));
                    }
                }
                stack[tip].label_end = i;
                stack[tip].target = Some(i + 2);
                destination_owner = Some(tip);
                i += 2;
                continue;
            }
            if bytes.get(i + 1) == Some(&b'[') {
                let mut end = i + 2;
                while end < bytes.len() && bytes[end] != b']' {
                    if bytes[end] == b'\\' {
                        end += 1;
                    }
                    end += 1;
                }
                if bytes.get(end) == Some(&b']') {
                    stack.pop();
                    i = end;
                }
                i += 1;
                continue;
            }
            if bytes.get(i + 1) == Some(&b'{') && read_djot_word_attributes(source, i + 1).is_some()
            {
                stack.pop();
            }
        }
        if bytes[i] == b'(' {
            if let Some(owner) = destination_owner {
                stack[owner].parens += 1;
            }
        }
        if bytes[i] != b')' || destination_owner.is_none() {
            i += 1;
            continue;
        }
        let owner_index = destination_owner.unwrap();
        let owner = stack[owner_index].clone();
        if owner.parens > 0 {
            stack[owner_index].parens -= 1;
            i += 1;
            continue;
        }
        if tip != owner_index {
            edits.insert(owner.at, (owner.at + 1, "\\[".to_owned()));
            stack.clear();
            pending_notes.clear();
            destination_owner = None;
            i += 1;
            continue;
        }
        let mut label = Vec::new();
        let mut brackets = 0;
        let mut at = owner.at + 1;
        while at < owner.label_end {
            if let Some((end, text)) = edits.get(&at) {
                if *end <= owner.label_end {
                    label.extend_from_slice(text.as_bytes());
                    at = *end;
                    continue;
                }
            }
            if let Some(&end) = angles.get(&at) {
                label.extend_from_slice(&bytes[at..end]);
                at = end;
                continue;
            }
            if mask.as_bytes()[at] == b' ' {
                label.push(bytes[at]);
                at += 1;
                continue;
            }
            if bytes[at] == b'\\' {
                let end = (at + 2).min(bytes.len());
                label.extend_from_slice(&bytes[at..end]);
                at = end;
                continue;
            }
            if bytes[at] == b'[' {
                brackets += 1;
            }
            if bytes[at] == b']' {
                if brackets > 0 {
                    brackets -= 1;
                } else {
                    label.push(b'\\');
                }
            }
            label.push(bytes[at]);
            at += 1;
        }
        let mut raw_destination = String::new();
        let mut at = owner.target.unwrap();
        while at < i {
            let end = if rows.get(line) != Some(&true) {
                literal_notes.get(&at).copied()
            } else {
                None
            };
            if let Some(end) = end.filter(|&end| end <= i) {
                raw_destination.push_str(
                    &note_newline.replace_all(&source[at..end].replace('\\', "%5C"), " "),
                );
                at = end;
            } else {
                let ch = source[at..].chars().next().unwrap();
                raw_destination.push(ch);
                at += ch.len_utf8();
            }
        }
        let raw_destination = escaped_char
            .replace_all(&raw_destination, |c: &regex::Captures<'_>| {
                if c[0].ends_with('\n') {
                    "\n".to_owned()
                } else {
                    c[0].to_owned()
                }
            })
            .into_owned();
        let mut destination = folds
            .replace_all(&raw_destination, |c: &regex::Captures<'_>| {
                let mut rest = c[1].trim_start_matches([' ', '\t']);
                for _ in 0..owner.depth {
                    if !rest.starts_with('>')
                        || rest
                            .as_bytes()
                            .get(1)
                            .is_some_and(|b| !matches!(b, b' ' | b'\t'))
                    {
                        break;
                    }
                    let Some(tail) = rest.strip_prefix('>') else {
                        break;
                    };
                    rest = tail.trim_start_matches([' ', '\t']);
                }
                rest.to_owned()
            })
            .into_owned();
        if rows.get(line) == Some(&true) {
            destination = escaped_pipe
                .replace_all(&destination, |c: &regex::Captures<'_>| {
                    format!("{}%7C", "%5C".repeat((c[0].len() - 1) / 2))
                })
                .into_owned();
        }
        if owner.at > 0
            && bytes[owner.at - 1] == b'!'
            && !is_escaped(bytes, owner.at - 1)
            && label.contains(&b'[')
        {
            let raw = String::from_utf8(label).expect("image labels preserve UTF-8");
            let converted = djot_to_carve(&format!("DJOTALT {raw} DJOTEND"));
            let plain = crate::to_plain_text_with_options(
                &converted,
                &crate::Options {
                    smart_typography: crate::SmartTypographyMode::Source,
                    ..Default::default()
                },
            );
            let plain = alt_end.replace(&plain, "");
            label = plain[8..]
                .replace('\\', "\\\\")
                .replace('[', "\\[")
                .replace(']', "\\]")
                .into_bytes();
        }
        let mut target = String::new();
        let mut chars = destination.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\\'
                && chars
                    .peek()
                    .is_some_and(|next| next.is_ascii_punctuation() || matches!(next, ' ' | '\t'))
            {
                let next = chars.next().unwrap();
                if "()\\".contains(next) {
                    target.push(ch);
                    target.push(next);
                } else if " \t\"<>`".contains(next) {
                    target.push_str(&format!("%{:02X}", next as u8));
                } else {
                    target.push(next);
                }
            } else if ch == '\\' {
                target.push_str("\\\\");
            } else if ch.is_whitespace()
                || "\"<>`()".contains(ch)
                || (ch == '|' && rows.get(line) == Some(&true))
            {
                for byte in ch.to_string().bytes() {
                    target.push_str(&format!("%{byte:02X}"));
                }
            } else {
                target.push(ch);
            }
        }
        edits.insert(
            owner.at,
            (
                i + 1,
                format!(
                    "[{}]({target})",
                    String::from_utf8(label).expect("link labels preserve UTF-8")
                ),
            ),
        );
        stack.pop();
        destination_owner = None;
        for at in pending_notes.drain() {
            edits.insert(at, (at + 1, "\\[".to_owned()));
        }
        i += 1;
    }
    let mut output = Vec::new();
    i = 0;
    while i < bytes.len() {
        if let Some((end, text)) = edits.get(&i) {
            output.extend_from_slice(text.as_bytes());
            i = *end;
        } else {
            output.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(output).expect("link replacements preserve UTF-8")
}

fn normalize_djot_attribute_lines(source: &str) -> String {
    if !source.contains('{') {
        return source.to_owned();
    }
    let mask = mask_djot_forms(source, true, false, None, &[]);
    let mut closes = std::collections::HashMap::new();
    let mut close = None;
    for (at, byte) in source.bytes().enumerate().rev() {
        match byte {
            b'\n' => close = None,
            b'}' => close = Some(at + 1),
            b'{' => {
                if let Some(end) = close {
                    closes.insert(at, end);
                }
            }
            _ => {}
        }
    }
    let mut output = String::new();
    let mut copied = 0;
    let mut i = 0;
    let attribute_key = cached_regex!(r"^[A-Za-z][A-Za-z0-9_-]*=").unwrap();
    while i < source.len() {
        if mask.as_bytes()[i] == b'{' && !is_escaped(source.as_bytes(), i) {
            if let Some((end, attrs)) = read_djot_word_attributes(source, i) {
                if source[i..end].contains('\n') {
                    output.push_str(&source[copied..i]);
                    output.push_str(&attrs);
                    copied = end;
                }
                i = end;
                continue;
            }
            if let Some(&end) = closes.get(&i) {
                if attribute_key.is_match(&source[i + 1..]) {
                    output.push_str(&source[copied..i]);
                    for ch in source[i..end].chars() {
                        if "{}[]".contains(ch) {
                            output.push('\\');
                        }
                        output.push(ch);
                    }
                    copied = end;
                    i = end;
                    continue;
                }
            }
        }
        i += 1;
    }
    output.push_str(&source[copied..]);
    output
}

fn fold_djot_references(source: &str) -> String {
    if !source.contains("]:") {
        return source.to_owned();
    }
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let mask = mask_djot_inline(source, false);
    let rows = djot_table_rows(source, &mask);
    let definition = cached_regex!(r"^\[([^\^\]\n][^\]\n]*|)\]:(?:[ \t]+(\S*)[ \t]*|)$").unwrap();
    let boundary = cached_regex!(r"^(?:#{1,6} |:{3,}|\{|\[[^\]]*\]:|(?:[-*][ \t]*){3,}$)").unwrap();
    let marker = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\)|:)[ \t]+").unwrap();
    let token = cached_regex!(r"^\S+$").unwrap();
    let thematic = cached_regex!(r"^(?:[ \t]*>[ \t]*)*[ \t]*(?:[-*][ \t]*){3,}$").unwrap();
    let mut removed = HashSet::new();
    let (mut offset, mut previous_depth, mut n) = (0, 0, 0);
    let mut previous = String::new();
    while n < lines.len() {
        let line = lines[n].clone();
        let depth = quoted(&line).0;
        let at = djot_content_start(&line);
        let content = &line[at..];
        let previous_line = n.checked_sub(1).map_or("", |n| lines[n].as_str());
        let previous_at = djot_content_start(previous_line);
        if let Some(caps) = definition.captures(content) {
            let allowed = previous.is_empty()
                || rows.get(n.wrapping_sub(1)) == Some(&true)
                || depth != previous_depth
                || thematic.is_match(previous_line)
                || at < previous_at && marker.is_match(&previous_line[..previous_at])
                || boundary.is_match(&previous)
                || marker.is_match(&line[..at]);
            if mask.as_bytes().get(offset + at) == Some(&b'[') && !allowed {
                lines[n] = line.replacen("]:", "]\\:", 1);
            }
            if mask.as_bytes().get(offset + at) == Some(&b'[') && allowed {
                let mut target = caps.get(2).map_or("", |m| m.as_str()).to_owned();
                let mut end = n;
                while end + 1 < lines.len() {
                    let next = &lines[end + 1];
                    let next_at = djot_content_start(next);
                    if quoted(next).0 != depth || next_at <= at || !token.is_match(&next[next_at..])
                    {
                        break;
                    }
                    target.push_str(&next[next_at..]);
                    end += 1;
                }
                if end > n {
                    lines[n] = format!("{}[{}]: {target}", &line[..at], &caps[1]);
                    for (k, value) in lines.iter_mut().enumerate().take(end + 1).skip(n + 1) {
                        offset += value.len() + 1;
                        removed.insert(k);
                    }
                    n = end;
                }
            }
        }
        previous = content.trim().to_owned();
        previous_depth = depth;
        offset += line.len() + 1;
        n += 1;
    }
    lines
        .into_iter()
        .enumerate()
        .filter_map(|(n, line)| (!removed.contains(&n)).then_some(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_djot_paragraph_fences(source: &str) -> String {
    use std::collections::HashMap;
    let rows = djot_table_rows(source, &mask_djot_inline(source, false));
    let mask = mask_djot_fences(source, None, &rows, true);
    if !mask.contains("```") && !mask.contains("~~~") {
        return source.to_owned();
    }
    let bytes = source.as_bytes();
    let tick_pattern = cached_regex!(r"`+").unwrap();
    let matches: Vec<_> = tick_pattern.find_iter(&mask).collect();
    let mut runs = HashMap::new();
    let mut next = HashMap::new();
    for m in matches.into_iter().rev() {
        let width = m.len();
        runs.insert(
            m.start(),
            (width, *next.get(&width).unwrap_or(&source.len())),
        );
        next.insert(width, m.start());
    }
    let breaks: Vec<_> = cached_regex!(r"\n[ \t]*(?:>[ \t]*)*\n")
        .unwrap()
        .find_iter(&mask)
        .map(|m| m.start())
        .collect();
    let angles: HashMap<_, _> = cached_regex!(r"<[^<>\s]+>")
        .unwrap()
        .find_iter(&mask)
        .filter(|m| {
            cached_regex!(r"[^:]@|[A-Za-z]:")
                .unwrap()
                .is_match(m.as_str())
        })
        .map(|m| (m.start(), m.end()))
        .collect();
    let head_pattern = cached_regex!(r"^[ \t]*(?:> ?[ \t]*)*").unwrap();
    let mut heads = HashSet::new();
    let mut line_offset = 0;
    for line in source.split('\n') {
        heads.insert(line_offset + head_pattern.find(line).unwrap().len());
        line_offset += line.len() + 1;
    }
    let mut output = String::new();
    let (mut copied, mut boundary, mut i) = (0, 0, 0);
    let tilde = cached_regex!(r"^~{3,}[ \t]*=?[A-Za-z0-9_+#.-]*[ \t]*(?:\n|$)").unwrap();
    let payload_end = cached_regex!(r"\n[ \t]*(?:>[ \t]*)*$").unwrap();
    while i < source.len() {
        if mask.as_bytes()[i] == b' ' {
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if let Some(&end) = angles.get(&i) {
            i = end;
            continue;
        }
        while breaks.get(boundary).is_some_and(|&end| end <= i) {
            boundary += 1;
        }
        if let Some(&(width, closer)) = runs.get(&i) {
            let end = closer.min(*breaks.get(boundary).unwrap_or(&source.len()));
            let closed = closer < *breaks.get(boundary).unwrap_or(&source.len());
            let raw_payload = &source[i + width..end];
            let payload = if !closed && end == source.len() {
                payload_end.replace(raw_payload, "")
            } else {
                std::borrow::Cow::Borrowed(raw_payload)
            };
            if width >= 3 && heads.contains(&i) {
                let widths: HashSet<_> =
                    tick_pattern.find_iter(&payload).map(|m| m.len()).collect();
                let mut native_width = 1;
                while widths.contains(&native_width) {
                    native_width += 1;
                }
                let ticks = "`".repeat(native_width);
                output.push_str(&source[copied..i]);
                if native_width >= 3 {
                    output.push_str("{%%}");
                }
                if payload.is_empty() {
                    output.push_str("`<code></code>`{=html}");
                } else {
                    output.push_str(&ticks);
                    output.push_str(&payload);
                    output.push_str(&ticks);
                }
                copied = end + if closed { width } else { 0 };
            }
            i = end + if closed { width } else { 0 };
            continue;
        }
        if bytes[i] == b'~' && heads.contains(&i) {
            if let Some(m) = tilde.find(&source[i..]) {
                let text = m.as_str().trim_end_matches('\n');
                output.push_str(&source[copied..i]);
                output.push('\\');
                output.push_str(text);
                copied = i + text.len();
                i = copied;
                continue;
            }
        }
        i += 1;
    }
    output.push_str(&source[copied..]);
    output
}

fn djot_note_alias(
    key: &str,
    labels: &mut std::collections::HashMap<String, String>,
    serial: &mut usize,
    reserved: &HashSet<usize>,
) -> String {
    labels
        .entry(key.to_owned())
        .or_insert_with(|| {
            while reserved.contains(serial) {
                *serial += 1;
            }
            let value = format!("carve-djot-note-{serial}");
            *serial += 1;
            value
        })
        .clone()
}

fn normalize_djot_footnotes(source: &str) -> String {
    use std::collections::HashMap;
    if !source.contains("[^") {
        return source.to_owned();
    }
    let mask = mask_djot_inline(source, false);
    let rows = djot_table_rows(source, &mask);
    let bytes = source.as_bytes();
    let mut non_rows = HashSet::new();
    let mut row_offset = 0;
    for (n, line) in source.split('\n').enumerate() {
        let at = djot_content_start(line);
        if rows.get(n) != Some(&true)
            && line.as_bytes().get(at) == Some(&b'|')
            && mask.as_bytes().get(row_offset + at) == Some(&b'|')
        {
            non_rows.insert(row_offset + at);
        }
        row_offset += line.len() + 1;
    }
    let mut labels = HashMap::new();
    let mut definitions = HashMap::new();
    let mut defined = HashSet::new();
    let mut used = Vec::new();
    let mut used_keys = HashSet::new();
    let reserved: HashSet<usize> = cached_regex!(r"(?i)carve-djot-note-(\d+)")
        .unwrap()
        .captures_iter(source)
        .filter_map(|c| c[1].parse().ok())
        .collect();
    let mut serial = 0;
    let mut offset = 0;
    let mut line_heads = HashSet::new();
    let mut empty_definitions = HashSet::new();
    let key_of = |key: &str| {
        cached_regex!(r"[ \t\r\n]+")
            .unwrap()
            .replace_all(
                key.trim_matches(|ch: char| {
                    (ch.is_whitespace() && ch != '\u{85}') || ch == '\u{feff}'
                }),
                " ",
            )
            .into_owned()
    };
    let unsupported = |key: &str| key.contains(['[', ']', '`', '<', '>', '|', '\\', '\u{c}', '\0']);
    let prefix =
        cached_regex!(r"^[ \t]*(?:>[ \t]*)*(?:(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\))[ \t]+)*")
            .unwrap();
    let head = cached_regex!(r"^\[\^([^\]\n]+)\]:(?:[ \t]|$)").unwrap();
    let thematic = cached_regex!(r"^(?:[ \t]*>[ \t]*)*[ \t]*(?:[-*][ \t]*){3,}$").unwrap();
    let note_lines: Vec<_> = source.split('\n').collect();
    let item = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\))[ \t]+").unwrap();
    let block = cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\{|\[[^\]]*\]:|(?:[-*][ \t]*){3,}$)")
        .unwrap();
    for (n, line) in note_lines.iter().enumerate() {
        let at = prefix.find(line).unwrap().len();
        line_heads.insert(offset + at);
        let previous_line = n.checked_sub(1).map_or("", |n| note_lines[n]);
        let previous_prefix = prefix.find(previous_line).unwrap().as_str();
        let previous = previous_line[previous_prefix.len()..].trim();
        let boundary = previous.is_empty()
            || thematic.is_match(previous_line)
            || at < previous_prefix.len() && item.is_match(previous_prefix)
            || n.checked_sub(1).is_some_and(|n| rows.get(n) == Some(&true))
            || item.is_match(&line[..at])
            || line[..at].matches('>').count() < previous_prefix.matches('>').count()
            || block.is_match(previous);
        if let Some(caps) = head.captures(&line[at..]) {
            if boundary && mask.as_bytes().get(offset + at) == Some(&b'[') {
                let key = key_of(&caps[1]);
                defined.insert(key.clone());
                definitions.insert(offset + at, (key.clone(), offset + at + 2 + caps[1].len()));
                if line[at + caps.get(0).unwrap().len()..].trim().is_empty() {
                    empty_definitions.insert(offset + at);
                }
                if unsupported(&key) {
                    djot_note_alias(&key, &mut labels, &mut serial, &reserved);
                }
            }
        }
        offset += line.len() + 1;
    }
    let angles: HashMap<_, _> = cached_regex!(r"<[^<>\s]+>")
        .unwrap()
        .find_iter(source)
        .filter(|m| {
            cached_regex!(r"[^:]@|[A-Za-z]:")
                .unwrap()
                .is_match(m.as_str())
        })
        .map(|m| (m.start(), m.end()))
        .collect();
    let mut i = 0;
    let blank = cached_regex!(r"^\n[ \t]*(?:>[ \t]*)*\n").unwrap();
    let mut destinations = HashMap::new();
    let mut accepted_destinations = HashSet::new();
    let mut destination_line = 0;
    let mut parens = Vec::new();
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'\n' {
            destination_line += 1;
        }
        if rows.get(destination_line) == Some(&true)
            && bytes[i] == b'|'
            && (i == 0 || bytes[i - 1] != b'\\')
        {
            parens.clear();
        }
        if bytes[i] == b'(' {
            parens.push(i);
        } else if bytes[i] == b')' {
            if let Some(at) = parens.pop() {
                if at > 0 && bytes[at - 1] == b']' {
                    destinations.insert(at, i + 1);
                }
            }
        }
        i += 1;
    }
    let mut malformed_ends = HashMap::new();
    let malformed_prefix = cached_regex!(r"^\{[A-Za-z][\w-]*=").unwrap();
    let mut next_brace = None;
    for at in (0..bytes.len()).rev() {
        if bytes[at] == b'\n' {
            next_brace = None;
        } else if bytes[at] == b'}' {
            next_brace = Some(at + 1);
        } else if bytes[at] == b'{' && malformed_prefix.is_match(&source[at..]) {
            if let Some(end) = next_brace {
                malformed_ends.insert(at, end);
            }
        }
    }
    let mut brackets = HashMap::new();
    let mut stack = Vec::new();
    let mut line_index = 0;
    i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            line_index += 1;
            if blank.is_match(&source[i..]) {
                stack.clear();
            }
        }
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(source, i) {
                i = end;
                continue;
            }
            if let Some(&end) = malformed_ends.get(&i) {
                i = end;
                continue;
            }
        }
        if let Some(&end) = angles.get(&i) {
            i = end;
            continue;
        }
        if mask.as_bytes()[i] == b' ' {
            i += 1;
            continue;
        }
        if rows.get(line_index) == Some(&true)
            && bytes[i] == b'|'
            && i.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\')
        {
            stack.clear();
            i += 1;
            continue;
        }
        if let Some(definition) = definitions.get(&i) {
            i = definition.1 + 1;
            continue;
        }
        if bytes[i] == b'[' {
            stack.push(i);
        } else if bytes[i] == b']' {
            if let Some(&open) = stack.last() {
                if bytes.get(open + 1) == Some(&b'^') {
                    brackets.insert(stack.pop().unwrap(), i);
                } else if bytes.get(i + 1) == Some(&b'(') && destinations.contains_key(&(i + 1)) {
                    brackets.insert(stack.pop().unwrap(), i);
                    accepted_destinations.insert(i + 1);
                    i = destinations[&(i + 1)];
                    continue;
                } else if bytes.get(i + 1) == Some(&b'[') {
                    let mut end = i + 2;
                    while end < bytes.len() && bytes[end] != b']' {
                        if bytes[end] == b'\\' {
                            end += 1;
                        }
                        end += 1;
                    }
                    if bytes.get(end) == Some(&b']') {
                        brackets.insert(stack.pop().unwrap(), i);
                        i = end;
                    }
                } else if bytes.get(i + 1) == Some(&b'{')
                    && read_djot_word_attributes(source, i + 1).is_some()
                {
                    brackets.insert(stack.pop().unwrap(), i);
                }
            }
        }
        i += 1;
    }
    let image_ends: HashMap<_, _> = brackets
        .iter()
        .filter(|(&at, &close)| {
            at > 0 && bytes[at - 1] == b'!' && matches!(bytes.get(close + 1), Some(b'[' | b'('))
        })
        .map(|(&at, &close)| (at, close))
        .collect();
    let mut output = Vec::new();
    i = 0;
    while i < source.len() {
        if let Some(&end) = destinations
            .get(&i)
            .filter(|_| accepted_destinations.contains(&i))
        {
            output.extend_from_slice(&bytes[i..end]);
            i = end;
            continue;
        }
        if non_rows.contains(&i) {
            output.push(b'\\');
        }
        if let Some(&end) = image_ends.get(&i) {
            let mut cursor = i;
            let mut at = i + 1;
            while at < end {
                if bytes[at..].starts_with(b"[^") {
                    if let Some(&close) = brackets.get(&at) {
                        if close < end {
                            output.extend_from_slice(&bytes[cursor..at]);
                            cursor = close + 1;
                            at = close;
                        }
                    }
                }
                at += 1;
            }
            output.extend_from_slice(&bytes[cursor..=end]);
            i = end + 1;
            continue;
        }
        if bytes[i] == b'{' && read_djot_word_attributes(source, i).is_none() {
            if let Some(&end) = malformed_ends.get(&i) {
                output.extend_from_slice(&bytes[i..end]);
                i = end;
                continue;
            }
        }
        if bytes[i] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(source, i) {
                output.extend_from_slice(&bytes[i..end]);
                i = end;
                continue;
            }
        }
        let definition = definitions.get(&i);
        let end = definition
            .map(|d| d.1)
            .or_else(|| brackets.get(&i).copied());
        if bytes[i..].starts_with(b"[^") && (definition.is_some() || mask.as_bytes()[i] == b'[') {
            if let Some(end) = end {
                let key = definition.map_or_else(|| key_of(&source[i + 2..end]), |d| d.0.clone());
                {
                    let rename =
                        labels.contains_key(&key) || unsupported(&key) || !defined.contains(&key);
                    let name = if rename {
                        djot_note_alias(&key, &mut labels, &mut serial, &reserved)
                    } else {
                        key.clone()
                    };
                    let at = i;
                    if definition.is_none() && used_keys.insert(key.clone()) {
                        used.push(key.clone());
                    }
                    if rename || key != source[i + 2..end] {
                        output.extend_from_slice(format!("[^{name}]").as_bytes());
                    } else {
                        output.extend_from_slice(&bytes[i..=end]);
                    }
                    i = end + 1;
                    if definition.is_some() && empty_definitions.contains(&at) {
                        output.extend_from_slice(b": %%%%");
                        i += 1;
                    }
                    if definition.is_none()
                        && bytes.get(i) == Some(&b':')
                        && line_heads.contains(&at)
                    {
                        output.extend_from_slice(b"\\:");
                        i += 1;
                    }
                    continue;
                }
            }
        }
        output.push(bytes[i]);
        i += 1;
    }
    let mut stubs = String::new();
    for key in used {
        if !defined.contains(&key) {
            let alias = djot_note_alias(&key, &mut labels, &mut serial, &reserved);
            stubs.push_str(&format!("[^{alias}]: %%%%\n\n"));
        }
    }
    stubs.push_str(&String::from_utf8(output).expect("footnote replacements preserve UTF-8"));
    stubs
}

fn normalize_djot_fences(source: &str) -> String {
    if !source.contains("```") && !source.contains("~~~") {
        return if source.contains("\\|") {
            close_djot_table_code(source)
        } else {
            source.to_owned()
        };
    }
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let rows = djot_table_rows(source, &mask_djot_inline(source, false));
    mask_djot_forms(
        source,
        false,
        false,
        Some(&mut |line, replacement| {
            lines[line] = replacement.to_owned();
        }),
        &rows,
    );
    let normalized = normalize_djot_paragraph_fences(&lines.join("\n"));
    let indented_ticks = cached_regex!(r"^[ \t]+`{3,}").unwrap();
    if source.contains("\\|")
        || source
            .lines()
            .any(|line| indented_ticks.is_match(quoted(line).1))
    {
        close_djot_table_code(&normalized)
    } else {
        normalized
    }
}

fn normalize_djot_autolinks(source: &str) -> String {
    if !source.contains('<') {
        return source.to_owned();
    }
    let bytes = source.as_bytes();
    let code_mask = mask_djot_inline(source, false);
    let mut mask = mask_code_and_destinations(source).into_bytes();
    let rows = djot_table_rows(source, &code_mask);
    let definition_pattern = cached_regex!(r"^\[[^\^\]\n][^\]\n]*\]:|^\[\]:").unwrap();
    let boundary_pattern =
        cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\{[.#A-Za-z]|\[[^\^\]][^\]]*\]:|\[\]:)")
            .unwrap();
    let continuation_pattern = cached_regex!(r"^\S+$").unwrap();
    let mut definition_indent = None;
    let mut definition_offset = 0;
    let mut previous_content = "";
    for (definition_line, line) in source.split('\n').enumerate() {
        let at = djot_content_start(line);
        let content = &line[at..];
        let boundary = previous_content.is_empty()
            || rows.get(definition_line.wrapping_sub(1)) == Some(&true)
            || boundary_pattern.is_match(previous_content);
        let definition = code_mask.as_bytes().get(definition_offset + at) == Some(&b'[')
            && definition_pattern.is_match(content)
            && boundary;
        let continuation = definition_indent.is_some_and(|indent| at > indent)
            && continuation_pattern.is_match(content);
        if definition || continuation {
            blank_out(&mut mask, definition_offset, definition_offset + line.len());
            if definition {
                definition_indent = Some(at);
            }
        } else {
            definition_indent = None;
        }
        previous_content = content.trim();
        definition_offset += line.len() + 1;
    }

    let valid_angle = cached_regex!(r"[^:]@|[A-Za-z]:").unwrap();
    let email = cached_regex!(r"[^:]@").unwrap();
    let authority_pattern = cached_regex!(r"^[A-Za-z][A-Za-z0-9+.-]*://[^/?#\\]*").unwrap();
    let angle_ends: std::collections::HashMap<usize, usize> = cached_regex!(r"<[^<>\s]+>")
        .unwrap()
        .find_iter(source)
        .filter(|value| valid_angle.is_match(value.as_str()))
        .map(|value| (value.start(), value.end()))
        .collect();
    let mut image_autolinks = HashSet::new();
    let mut paren_ends = std::collections::HashMap::new();
    let mut parens = Vec::new();
    let mut quote = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if let Some(ch) = quote {
            if bytes[i] == ch || bytes[i] == b'\n' {
                quote = None;
            }
        } else if !parens.is_empty()
            && i > 0
            && matches!(bytes[i - 1], b' ' | b'\t')
            && matches!(bytes[i], b'"' | b'\'')
        {
            quote = Some(bytes[i]);
        } else if bytes[i] == b'(' {
            parens.push(i);
        } else if bytes[i] == b')' {
            if let Some(start) = parens.pop() {
                paren_ends.insert(start, i);
            }
        }
        i += 1;
    }
    let mut bracket_ends = std::collections::HashMap::new();
    let mut nested_brackets = HashSet::new();
    let mut stack = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if let Some(end) = angle_ends.get(&i) {
            i = *end;
            continue;
        }
        if bytes[i] == b'[' {
            if let Some(parent) = stack.last() {
                nested_brackets.insert(*parent);
            }
            stack.push(i);
        }
        if bytes[i] == b']' {
            if let Some(start) = stack.pop() {
                bracket_ends.insert(start, i);
            }
        }
        i += 1;
    }
    i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        let mut end = None;
        if bytes[i] == b'{' && mask[i] == b'{' {
            end = read_djot_word_attributes(source, i).map(|value| value.0);
        }
        if bytes[i] == b'!' && mask[i] == b'!' && bytes.get(i + 1) == Some(&b'[') {
            if let Some(close) = bracket_ends.get(&(i + 1)) {
                if bytes
                    .get(close + 1)
                    .is_some_and(|byte| matches!(byte, b'(' | b'['))
                {
                    let mut angles = Vec::new();
                    let mut plain = true;
                    let mut at = i + 2;
                    while at < *close {
                        if let Some(end) = angle_ends.get(&at).filter(|end| **end <= *close) {
                            angles.push(at);
                            at = *end;
                            continue;
                        }
                        if b"`{_*~^\\[".contains(&bytes[at]) {
                            plain = false;
                            break;
                        }
                        at += 1;
                    }
                    if plain {
                        image_autolinks.extend(angles);
                    }
                    end = Some(close + 1);
                }
            }
        }
        if bytes[i] == b']' && bytes.get(i + 1) == Some(&b'[') {
            if let Some(close) = bracket_ends.get(&(i + 1)) {
                end = Some(close + 1);
            }
        }
        if bytes[i] == b']' && bytes.get(i + 1) == Some(&b'(') {
            if let Some(close) = paren_ends.get(&(i + 1)) {
                end = Some(close + 1);
            }
        }
        if bytes[i] == b'[' && bytes.get(i + 1) == Some(&b'^') {
            if let Some(close) = bracket_ends.get(&i) {
                if !nested_brackets.contains(&i) {
                    end = Some(close + 1);
                }
            }
        }
        if let Some(end) = end {
            blank_out(&mut mask, i, end);
            i = end;
        } else {
            i += 1;
        }
    }
    let mut out = String::with_capacity(source.len());
    let mut copied = 0;
    let mut line = 0;
    let mut offset = 0;
    for capture in cached_regex!(r"<([^<>\s]+)>")
        .unwrap()
        .captures_iter(source)
    {
        let matched = capture.get(0).unwrap();
        let at = matched.start();
        while offset < at {
            if bytes[offset] == b'\n' {
                line += 1;
            }
            offset += 1;
        }
        let image = image_autolinks.contains(&at) && code_mask.as_bytes()[at] == b'<';
        if (!image && mask[at] != b'<') || is_escaped(bytes, at) {
            continue;
        }
        let body = capture[1].to_owned();
        if !valid_angle.is_match(&body)
            || (!image
                && !body.contains(['[', ']', '`', '|', '\\'])
                && !(email.is_match(&body) && body.contains(':')))
        {
            continue;
        }
        if rows[line] && body.contains(['|', '`']) {
            continue;
        }
        let mut label = String::new();
        for ch in body.chars() {
            if ch.is_ascii_punctuation() {
                label.push('\\');
            }
            label.push(ch);
        }
        let destination = if email.is_match(&body) {
            format!("mailto:{body}")
        } else {
            body.clone()
        };
        let authority = authority_pattern
            .find(&destination)
            .map_or(0, |value| value.end());
        let mut target = String::new();
        for (at, ch) in destination.char_indices() {
            match ch {
                '`' => target.push_str("%60"),
                '|' => target.push_str("%7C"),
                '\\' => target.push_str("\\\\"),
                '[' if at >= authority => target.push_str("%5B"),
                ']' if at >= authority => target.push_str("%5D"),
                '(' => target.push_str("%28"),
                ')' => target.push_str("%29"),
                _ => target.push(ch),
            }
        }
        out.push_str(&source[copied..at]);
        if image {
            out.push_str(&label);
        } else {
            out.push_str(&format!("[{label}]({target})"));
        }
        copied = matched.end();
    }
    out.push_str(&source[copied..]);
    out
}

fn djot_content_start(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut at = 0;
    let marker_re = cached_regex!(
        r"^(?:\[\^[^\]\n]+\]:[ \t]*|(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\)|:)[ \t]+)"
    )
    .unwrap();
    while at < bytes.len() {
        while matches!(bytes.get(at), Some(b' ' | b'\t')) {
            at += 1;
        }
        if bytes.get(at) == Some(&b'>') {
            at += 1;
            continue;
        }
        let marker = marker_re.find(&line[at..]);
        match marker {
            Some(marker) => at += marker.end(),
            None => break,
        }
    }
    at
}

fn djot_table_rows(source: &str, mask: &str) -> Vec<bool> {
    let item_re = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\)|:)[ \t]+").unwrap();
    let boundary_re = cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\{|\[[^\]]+\]:)").unwrap();
    let mut offset = 0;
    let mut previous_row = false;
    let mut previous_block = true;
    let mut footnote_column = None;
    let note = cached_regex!(
        r"^[ \t]*(?:>[ \t]*)*(?:(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\))[ \t]+)*\[\^[^\]\n]+\]:"
    )
    .unwrap();
    source
        .split('\n')
        .map(|line| {
            let at = djot_content_start(line);
            let bytes = line.as_bytes();
            let end = line.trim_end().len().saturating_sub(1);
            let content = &line[at..];
            let closes_note = footnote_column.is_some_and(|column| at < column);
            if closes_note {
                footnote_column = None;
            }
            let allowed =
                previous_block || previous_row || item_re.is_match(&line[..at]) || closes_note;
            if allowed {
                if let Some(m) = note.find(line) {
                    footnote_column = Some(m.as_str().find("[^").unwrap() + 2);
                }
            }
            let row = allowed
                && bytes.get(at) == Some(&b'|')
                && mask.as_bytes().get(offset + at) == Some(&b'|')
                && bytes.get(end) == Some(&b'|')
                && mask.as_bytes().get(offset + end) == Some(&b'|')
                && end.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\');
            previous_block = content.trim().is_empty() || boundary_re.is_match(content);
            previous_row = row;
            offset += line.len() + 1;
            row
        })
        .collect()
}

fn rename_djot_pipe_footnotes(source: &str) -> String {
    let mask = mask_djot_inline(source, false);
    let prefix = "carve-djot-footnote-";
    let reserved: std::collections::HashSet<usize> =
        cached_regex!(r"(?i)carve-djot-footnote-(\d+)")
            .unwrap()
            .captures_iter(source)
            .filter_map(|caps| caps[1].parse().ok())
            .collect();
    let mut serial = 0;
    let spaces = cached_regex!(r"\s+").unwrap();
    let mut labels = std::collections::HashMap::new();
    let mut definition_ends = std::collections::HashMap::new();
    let definitions = cached_regex!(
        r"(?m)^[ \t]*(?:>[ \t]*)*(?:(?:[-*+]|[0-9A-Za-z]+[.)])[ \t]+)?\[\^([^\]\n]+)\]:"
    )
    .unwrap();
    for caps in definitions.captures_iter(source) {
        let value = caps.get(0).unwrap();
        let at = value.start() + value.as_str().find('[').unwrap();
        let key = spaces.replace_all(&caps[1], " ").trim().to_owned();
        if mask.as_bytes()[at] == b'[' {
            definition_ends.insert(at, at + 2 + caps[1].len());
        }
        if mask.as_bytes()[at] == b'['
            && !key.contains('[')
            && key.contains('|')
            && !labels.contains_key(&key)
        {
            while reserved.contains(&serial) {
                serial += 1;
            }
            labels.insert(key, format!("{prefix}{serial}"));
            serial += 1;
        }
    }
    if labels.is_empty() && !source.contains("[^") {
        return source.to_owned();
    }
    let item_re = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\)|:)[ \t]+").unwrap();
    let boundary_re = cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\{|\[[^\]]+\]:)").unwrap();
    let mut parens = std::collections::HashMap::new();
    let mut stack = Vec::new();
    let mut row_ranges = Vec::new();
    let mut line_offset = 0;
    let mut previous_row = false;
    let mut previous_block = true;
    for line in source.split('\n') {
        let at = djot_content_start(line);
        let bytes = line.as_bytes();
        let end = line.trim_end().len().saturating_sub(1);
        let content = &line[at..];
        let row = (previous_block || previous_row || item_re.is_match(&line[..at]))
            && bytes.get(at) == Some(&b'|')
            && mask.as_bytes().get(line_offset + at) == Some(&b'|')
            && bytes.get(end) == Some(&b'|')
            && mask.as_bytes().get(line_offset + end) == Some(&b'|')
            && end.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\');
        previous_block = content.trim().is_empty() || boundary_re.is_match(content);
        previous_row = row;
        if row {
            row_ranges.push((line_offset + at, line_offset + bytes.len()));
        }
        if row || content.trim().is_empty() {
            stack.clear();
        }
        let mut i = 0;
        while i < bytes.len() {
            if mask.as_bytes()[line_offset + i] == b' ' {
                i += 1;
                continue;
            }
            if bytes[i] == b'{' {
                if let Some((end, _)) = read_djot_word_attributes(line, i) {
                    i = end;
                    continue;
                }
            }
            if bytes[i] == b'\\' && bytes.get(i + 1).is_some_and(u8::is_ascii_punctuation) {
                i += 2;
                continue;
            }
            if row
                && bytes[i] == b'|'
                && i.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\')
            {
                stack.clear();
            } else if bytes[i] == b'(' {
                stack.push(line_offset + i);
            } else if bytes[i] == b')' {
                if let Some(open) = stack.pop() {
                    parens.insert(open, line_offset + i);
                }
            }
            i += 1;
        }
        if row {
            stack.clear();
        }
        line_offset += line.len() + 1;
    }
    let mut row_index = 0;
    let bytes = source.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut images = Vec::new();
    let mut image_depth = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let mut next = i + 1;
            while matches!(bytes.get(next), Some(b' ' | b'\t')) {
                next += 1;
            }
            if bytes.get(next) == Some(&b'\n') {
                images.clear();
                image_depth = 0;
            }
        }
        if mask.as_bytes()[i] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(source, i) {
                output.extend_from_slice(&bytes[i..end]);
                i = end;
                continue;
            }
        }
        if mask.as_bytes()[i] == b' ' {
            output.push(bytes[i]);
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' {
            output.push(bytes[i]);
            i += 1;
            if bytes.get(i).is_some_and(u8::is_ascii_punctuation) {
                output.push(bytes[i]);
                i += 1;
            }
            continue;
        }
        while row_ranges.get(row_index).is_some_and(|(_, end)| *end <= i) {
            row_index += 1;
        }
        let in_row = row_ranges
            .get(row_index)
            .is_some_and(|(start, _)| *start <= i);
        if in_row && bytes[i] == b'|' && i.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\')
        {
            images.clear();
            image_depth = 0;
        }
        if bytes[i..].starts_with(b"[^") {
            let mut end = i + 2;
            while end < bytes.len() && !matches!(bytes[end], b'[' | b']' | b'\n') {
                end += 1;
            }
            if bytes.get(end) == Some(&b'[') {
                if let Some(&definition_end) = definition_ends.get(&i) {
                    output.extend_from_slice(&bytes[i..=definition_end]);
                    i = definition_end + 1;
                    continue;
                }
                output.extend_from_slice(b"\\[^");
                i += 2;
                continue;
            }
            if bytes.get(end) == Some(&b']') {
                let label = &source[i + 2..end];
                let raw_pipe = in_row
                    && label.as_bytes().iter().enumerate().any(|(at, &ch)| {
                        ch == b'|' && (at == 0 || label.as_bytes()[at - 1] != b'\\')
                    });
                if raw_pipe {
                    images.clear();
                    image_depth = 0;
                }
                let renamed = if image_depth == 0 && !raw_pipe {
                    labels.get(spaces.replace_all(&source[i + 2..end], " ").trim())
                } else {
                    None
                };
                match renamed {
                    Some(label) => output.extend_from_slice(format!("[^{label}]").as_bytes()),
                    None => output.extend_from_slice(&bytes[i..=end]),
                }
                i = end + 1;
                continue;
            }
        }
        if bytes[i] == b'[' {
            let image = i > 0 && bytes[i - 1] == b'!';
            images.push(image);
            if image {
                image_depth += 1;
            }
        } else if bytes[i] == b']' && !images.is_empty() {
            if images.pop() == Some(true) {
                image_depth -= 1;
            }
            if bytes.get(i + 1) == Some(&b'(') {
                if let Some(&end) = parens.get(&(i + 1)) {
                    output.extend_from_slice(&bytes[i..=end]);
                    i = end + 1;
                    continue;
                }
            }
        }
        output.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(output).expect("only ASCII syntax changed")
}

fn close_djot_table_code(source: &str) -> String {
    if !source.contains('`') {
        return source.to_owned();
    }
    let bytes = source.as_bytes();
    let paragraph_ends: Vec<usize> = cached_regex!(r"\n[ \t]*(?:>[ \t]*)*\n")
        .unwrap()
        .find_iter(source)
        .map(|m| m.start())
        .collect();
    let ticks = cached_regex!(r"`+").unwrap();
    let mut runs: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for run in ticks.find_iter(source) {
        runs.entry(run.len()).or_default().push(run.start());
    }
    let close_run = |open: usize, width: usize| {
        runs.get(&width)
            .and_then(|positions| positions.get(positions.partition_point(|&at| at < open + width)))
            .copied()
            .unwrap_or(bytes.len())
    };
    let mut tick_starts = Vec::new();
    let mut tick_widths = Vec::new();
    let query_end = if source.contains("![") || source.contains("[^") || source.contains("](") {
        source
            .rfind(']')
            .unwrap_or(0)
            .max(source.rfind(')').unwrap_or(0))
    } else {
        0
    };
    let mut at = 0;
    while at < query_end {
        if bytes[at] == b'\\' && bytes.get(at + 1).is_some_and(u8::is_ascii_punctuation) {
            at += 2;
            continue;
        }
        if bytes[at] != b'`' {
            at += 1;
            continue;
        }
        let width = bytes[at..].iter().take_while(|&&b| b == b'`').count();
        tick_starts.push(at);
        tick_widths.push(width);
        at += width;
    }
    let count = tick_starts.len();
    let tick_index = |position: usize| tick_starts.partition_point(|&at| at < position);
    let mut next = vec![count; count + 1];
    let mut ends = vec![bytes.len() + 1; count + 1];
    let mut jumps = vec![count; count + 1];
    let mut depths = vec![0; count + 1];
    // Merge equal-length ancestor jumps so each tick stores one pointer.
    for at in (0..count).rev() {
        let finish = close_run(tick_starts[at], tick_widths[at]) + tick_widths[at];
        let parent = tick_index(finish);
        let jump = jumps[parent];
        let farther = jumps[jump];
        next[at] = parent;
        ends[at] = finish;
        depths[at] = depths[parent] + 1;
        jumps[at] = if depths[parent] - depths[jump] == depths[jump] - depths[farther] {
            farther
        } else {
            parent
        };
    }
    let balanced_ticks = |from: usize, end: usize| {
        let mut at = tick_index(from);
        while at < count && tick_starts[at] < end {
            let jump = jumps[at];
            if jump < count && ends[jump] <= end {
                at = jump;
            } else if ends[at] <= end {
                at = next[at];
            } else {
                return false;
            }
        }
        true
    };
    let mut parens = std::collections::HashMap::new();
    let mut labels = std::collections::HashMap::new();
    let mut parenthesis_stack = Vec::new();
    let mut bracket_stack = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\\' && bytes.get(at + 1).is_some_and(u8::is_ascii_punctuation) {
            at += 2;
            continue;
        }
        match bytes[at] {
            b'(' => parenthesis_stack.push(at),
            b')' => {
                if let Some(open) = parenthesis_stack.pop() {
                    parens.insert(open, at);
                }
            }
            b'[' => bracket_stack.push(at),
            b']' => {
                if let Some(open) = bracket_stack.pop() {
                    labels.insert(open, at);
                }
            }
            _ => {}
        }
        at += 1;
    }
    let block = cached_regex!(r"^(?:[-*+] |(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)] |\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\) |: |#{1,6} |`{3,}|~{3,}|:{3,}|>|\||\^ |\[[^\]]+\]:)").unwrap();
    let marker = cached_regex!(
        r"^(?:\[\^[^\]\n]+\]:[ \t]*|(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)|:)[ \t]+)"
    )
    .unwrap();
    let digits = cached_regex!(r"^[0-9]+").unwrap();
    let letters = cached_regex!(r"^[A-Za-z]+").unwrap();
    let item_kind_of = |item: &str| {
        letters
            .replace(&digits.replace(item, "1"), "a")
            .trim()
            .to_owned()
    };
    let autolink =
        cached_regex!(r"^<(?:[A-Za-z][A-Za-z0-9+.-]*:[^<>\s]*|[^<>\s@]+@[^<>\s]+)>").unwrap();
    let div_re = cached_regex!(r"^(:{3,})(?:[ \t]+\S.*)?[ \t]*$").unwrap();
    let close_div = cached_regex!(r"^:{3,}[ \t]*$").unwrap();
    let quote_re = cached_regex!(r"^[ \t]*>[ ]?").unwrap();
    let separator = cached_regex!(r"^\|[ \t:|-]*\|$").unwrap();
    let fence_open = cached_regex!(r"^(`{3,}|~{3,})[ \t]*=?[a-zA-Z0-9_+#.-]*$").unwrap();
    let boundary = cached_regex!(r"^(?:#{1,6} |:{3,}|\[[^\]]+\]:)").unwrap();
    let mut paragraph = 0;
    let mut offset = 0;
    let mut cursor = 0;
    let mut consumed = 0;
    let mut item_column = 0;
    let mut item_quote = 0;
    let mut previous_quote = 0;
    let mut item_kind = String::new();
    let mut previous_block = true;
    let mut fenced: Option<(usize, u8, usize, usize, usize)> = None;
    let mut divs: Vec<(usize, usize, usize)> = Vec::new();
    let mut output = String::new();
    for line in source.split('\n') {
        let at = djot_content_start(line);
        let line_end = offset + line.len();
        let (depth, content) = quoted(line);
        let trimmed = content.trim_start();
        let indent = content.len() - trimmed.len();
        let item = marker.find(trimmed);
        let old_column = item_column;
        let old_quote = item_quote;
        let began_inside = offset < consumed;
        if !began_inside {
            if fenced.is_some_and(|(_, _, owner_depth, _, owner_item)| {
                !trimmed.is_empty()
                    && (depth < owner_depth || (depth == owner_depth && indent < owner_item))
            }) {
                fenced = None;
            }
            if let Some((width, ch, owner_depth, column, _)) = fenced {
                let run = trimmed.bytes().take_while(|&b| b == ch).count();
                if depth == owner_depth
                    && indent <= column
                    && run >= width
                    && trimmed[run..].trim_matches([' ', '\t']).is_empty()
                {
                    fenced = None;
                    previous_block = true;
                }
                offset = line_end + 1;
                continue;
            }
            if let Some(opening) = fence_open
                .captures(&line[at..])
                .filter(|_| previous_block || item.is_some())
            {
                if item.is_none()
                    && (depth < item_quote || (depth == item_quote && indent < item_column))
                {
                    item_column = indent;
                    item_quote = depth;
                    item_kind.clear();
                }
                fenced = Some((
                    opening[1].len(),
                    opening[1].as_bytes()[0],
                    depth,
                    at,
                    item.map_or(item_column.min(indent), |item| {
                        indent
                            + if item.as_str().starts_with("[^") {
                                2
                            } else {
                                item.len()
                            }
                    }),
                ));
                offset = line_end + 1;
                continue;
            }
            if let Some(item) = item {
                item_column = indent
                    + if item.as_str().starts_with("[^") {
                        2
                    } else {
                        item.len()
                    };
                item_quote = depth;
                item_kind = item_kind_of(item.as_str());
            } else if !trimmed.is_empty()
                && (depth < item_quote
                    || (depth == item_quote
                        && indent < item_column
                        && (block.is_match(trimmed)
                            || (trimmed.starts_with('{')
                                && read_djot_word_attributes(trimmed, 0).is_some()))))
            {
                item_column = 0;
            }
            if let Some(div) = div_re.captures(&line[at..]) {
                if divs.last().is_some_and(|&(width, owner_depth, _)| {
                    depth == owner_depth && close_div.is_match(&line[at..]) && div[1].len() >= width
                }) {
                    divs.pop();
                } else {
                    divs.push((div[1].len(), depth, at));
                }
            }
            if line.as_bytes().get(at) == Some(&b'|')
                && ((old_column > 0 && depth <= old_quote && indent < old_column)
                    || depth < previous_quote)
            {
                output.push_str(&source[cursor..offset]);
                output.push('\n');
                cursor = offset;
            }
            previous_quote = depth;
        }
        let mut i = (offset + at).max(consumed);
        let mut pending = Vec::new();
        let mut footnotes = Vec::new();
        while i < line_end {
            if bytes[i] == b'\\' && bytes.get(i + 1).is_some_and(u8::is_ascii_punctuation) {
                i += 2;
                continue;
            }
            if bytes[i] == b'{' {
                if let Some((end, _)) = read_djot_word_attributes(source, i) {
                    i = end;
                    continue;
                }
            }
            if bytes[i] == b'<' {
                if let Some(auto) = autolink.find(&source[i..]) {
                    i += auto.len();
                    continue;
                }
            }
            if bytes[i..].starts_with(b"![") {
                if let Some(&end) = labels
                    .get(&(i + 1))
                    .filter(|&&end| balanced_ticks(i + 2, end))
                {
                    i = end + 1;
                    continue;
                }
            }
            if bytes[i..].starts_with(b"[^") {
                let mut end = i + 2;
                while end < line_end && !matches!(bytes[end], b'[' | b']') {
                    end += 1;
                }
                if bytes.get(end) == Some(&b']') && balanced_ticks(i + 2, end) {
                    i = end + 1;
                    continue;
                }
                footnotes.push(i);
            }
            if bytes[i..].starts_with(b"](") {
                if let Some(&end) = parens
                    .get(&(i + 1))
                    .filter(|&&end| end <= line_end && balanced_ticks(i + 2, end))
                {
                    i = end + 1;
                    continue;
                }
            }
            if bytes[i] == b']' {
                footnotes.clear();
            }
            if bytes[i] == b'(' {
                pending.push(i);
            } else if bytes[i] == b')' {
                pending.pop();
            }
            if bytes[i] != b'`' {
                i += source[i..].chars().next().unwrap().len_utf8();
                continue;
            }
            let width = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            let candidate = close_run(i, width);
            if candidate + width <= line_end {
                i = candidate + width;
                consumed = i;
                continue;
            }
            while paragraph_ends.get(paragraph).is_some_and(|&end| end <= i) {
                paragraph += 1;
            }
            let mut limit = candidate.min(
                paragraph_ends
                    .get(paragraph)
                    .copied()
                    .unwrap_or(bytes.len()),
            );
            let mut separate = false;
            let mut next = line_end + 1;
            while next < limit {
                let newline = source[next..].find('\n').map(|at| next + at);
                let end = newline.unwrap_or(bytes.len());
                let (next_depth, next_content) = quoted(&source[next..end]);
                let next_trimmed = next_content.trim_start();
                let next_indent = next_content.len() - next_trimmed.len();
                let closes_div = divs.last().is_some_and(|&(width, owner_depth, column)| {
                    next_depth == owner_depth
                        && next_indent <= column + 3
                        && close_div.is_match(next_trimmed)
                        && next_trimmed.trim().len() >= width
                });
                let outside = (depth > 0 && next_depth < depth)
                    || (item_column > 0 && next_depth <= item_quote && next_indent < item_column);
                let attributes = next_trimmed.starts_with('{')
                    && read_djot_word_attributes(next_trimmed, 0).is_some();
                if closes_div || (outside && (block.is_match(next_trimmed) || attributes)) {
                    limit = next - 1;
                    let next_item = marker
                        .find(next_trimmed)
                        .map(|item| item_kind_of(item.as_str()));
                    separate = outside
                        && (next_depth < depth || next_item.as_deref() != Some(item_kind.as_str()));
                    break;
                }
                let Some(newline) = newline else {
                    break;
                };
                next = newline + 1;
            }
            let closed = candidate <= limit && candidate < bytes.len();
            let end = if closed { candidate } else { limit };
            let mut payload = &source[i + width..end];
            payload = payload.strip_suffix('\n').unwrap_or(payload);
            if payload.starts_with(" `") {
                payload = &payload[1..];
            }
            if payload.ends_with("` ") {
                payload = &payload[..payload.len() - 1];
            }
            let normalized: Vec<String> = payload
                .split('\n')
                .enumerate()
                .map(|(n, value)| {
                    if n == 0 {
                        return value.trim_end_matches([' ', '\t']).to_owned();
                    }
                    let mut rest = value;
                    for _ in 0..depth {
                        let Some(prefix) = quote_re.find(rest) else {
                            break;
                        };
                        rest = &rest[prefix.end()..];
                    }
                    rest.trim_matches([' ', '\t']).to_owned()
                })
                .collect();
            let raw_code = normalized.iter().skip(1).any(|value| {
                (block.is_match(value) || value.starts_with('{'))
                    && (!separator.is_match(value) || line.as_bytes().get(at) != Some(&b'|'))
            }) || (item_kind.starts_with("[^") && normalized.len() > 1);
            let raw_payload = if raw_code {
                let mut html = String::from("<code>");
                for ch in normalized.join("\n").chars() {
                    if ch.is_ascii_punctuation() || ch == '\n' {
                        html.push_str(&format!("&#{};", ch as u32));
                    } else {
                        html.push(ch);
                    }
                }
                html.push_str("</code>");
                html
            } else {
                String::new()
            };
            if raw_code {
                payload = &raw_payload;
            }
            let fence_width = ticks
                .find_iter(payload)
                .map(|run| run.len() + 1)
                .max()
                .unwrap_or(1);
            let fence = "`".repeat(fence_width);
            let pad = if payload.starts_with('`')
                || payload.ends_with('`')
                || (payload.starts_with(' ')
                    && payload.ends_with(' ')
                    && !payload.trim().is_empty())
            {
                " "
            } else {
                ""
            };
            let mut escapes: Vec<usize> = pending
                .iter()
                .copied()
                .filter(|&k| k > 0 && bytes[k - 1] == b']')
                .chain(footnotes.iter().copied())
                .collect();
            escapes.sort_unstable();
            for escape in escapes {
                output.push_str(&source[cursor..escape]);
                output.push('\\');
                cursor = escape;
            }
            output.push_str(&source[cursor..i]);
            output.push_str(&format!(
                "{fence}{pad}{payload}{pad}{fence}{}",
                if raw_code { "{=html}" } else { "" }
            ));
            cursor = if closed { end + width } else { end };
            consumed = cursor;
            if separate && !closed {
                output.push('\n');
            }
            i = consumed;
        }
        previous_block = !began_inside
            && consumed <= line_end
            && (trimmed.is_empty()
                || boundary.is_match(trimmed)
                || (trimmed.starts_with('{') && read_djot_word_attributes(trimmed, 0).is_some())
                || (line.as_bytes().get(at) == Some(&b'|')
                    && line.trim_end().ends_with('|')
                    && !line.trim_end().ends_with("\\|")));
        offset = line_end + 1;
    }
    output.push_str(&source[cursor..]);
    output
}

fn normalize_djot_table_pipes(source: &str) -> String {
    if !source.contains("\\|") {
        return source.to_owned();
    }
    fn punctuation(byte: Option<&u8>) -> bool {
        byte.is_some_and(u8::is_ascii_punctuation)
    }
    use std::collections::HashMap;
    let renamed = rename_djot_pipe_footnotes(source);
    let source = renamed.as_str();
    let mask = mask_djot_inline(source, false);
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let mut offsets = Vec::with_capacity(lines.len());
    let mut offset = 0;
    for line in &lines {
        offsets.push(offset);
        offset += line.len() + 1;
    }
    let definition_re = cached_regex!(r"^\[([^\[\]\n^][^\[\]\n]*)\]:[ \t]*(\S*)[ \t]*$").unwrap();
    let spaces = cached_regex!(r"\s+").unwrap();
    let escapes = cached_regex!(r"\\([!-/:-@\[-`{-~])").unwrap();
    let item_re = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\)|:)[ \t]+").unwrap();
    let boundary_re = cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\[[^\]]+\]:)").unwrap();
    let attribute_pipes_re = cached_regex!(r"\\+\|?|\|").unwrap();
    let mut definitions: HashMap<String, (String, String)> = HashMap::new();
    let row_flags = djot_table_rows(source, &mask);
    let thematic_re = cached_regex!(r"^(?:\*[ \t]*){3,}$|^(?:-[ \t]*){3,}$").unwrap();
    for n in 0..lines.len() {
        let line = lines[n].clone();
        let at = djot_content_start(&line);
        if mask.as_bytes().get(offsets[n] + at) != Some(&b'[') {
            continue;
        }
        let Some(definition) = definition_re.captures(&line[at..]) else {
            continue;
        };
        if !definition[1].contains('|') {
            continue;
        }
        let previous_line = if n > 0 { lines[n - 1].as_str() } else { "" };
        let previous_at = djot_content_start(previous_line);
        let previous = previous_line[previous_at..].trim();
        let opens_item = item_re.is_match(&line[..at]);
        if !previous.is_empty()
            && !row_flags.get(n.wrapping_sub(1)).copied().unwrap_or(false)
            && at >= previous_at
            && !thematic_re.is_match(previous)
            && !boundary_re.is_match(previous)
            && !(previous.starts_with('{')
                && read_djot_word_attributes(previous, 0)
                    .is_some_and(|(end, _)| end == previous.len()))
            && !opens_item
            && line[..at].bytes().filter(|&b| b == b'>').count()
                <= previous_line[..previous_at]
                    .bytes()
                    .filter(|&b| b == b'>')
                    .count()
        {
            lines[n] = format!("{}\\{}", &line[..at], &line[at..]);
            continue;
        }
        let mut target = definition[2].to_owned();
        let mut end = n;
        while end + 1 < lines.len() {
            let next = &lines[end + 1];
            let next_at = djot_content_start(next);
            if next_at <= at
                || next[next_at..].is_empty()
                || next[next_at..].chars().any(char::is_whitespace)
                || mask.as_bytes().get(offsets[end + 1] + next_at) == Some(&b' ')
            {
                break;
            }
            target.push_str(&next[next_at..]);
            end += 1;
        }
        if target.is_empty() {
            continue;
        }
        let mut attribute_parts = Vec::new();
        for k in (0..n).rev() {
            let value = lines[k][djot_content_start(&lines[k])..].trim();
            let parsed = if value.starts_with('{') {
                read_djot_word_attributes(value, 0)
            } else {
                None
            };
            if !parsed.is_some_and(|(end, _)| end == value.len()) {
                break;
            }
            attribute_parts.push(value);
        }
        definitions.insert(
            spaces.replace_all(&definition[1], " ").trim().to_owned(),
            (target.clone(), attribute_parts.into_iter().rev().collect()),
        );
        lines[n] = format!("{}[{}]: {}", &line[..at], &definition[1], target);
        for continuation in &mut lines[n + 1..=end] {
            continuation.clear();
        }
    }
    let mut result = Vec::with_capacity(lines.len());
    for (n, line) in lines.iter().enumerate() {
        let at = djot_content_start(line);
        let bytes = line.as_bytes();
        let line_mask = mask
            .as_bytes()
            .get(offsets[n]..offsets[n] + line.len())
            .unwrap_or(&[]);
        let row = bytes.get(at) == Some(&b'|') && line_mask.get(at) == Some(&b'|');
        if !row {
            result.push(line.clone());
            continue;
        }
        if row_flags.get(n) != Some(&true) {
            result.push(format!("{}\\{}", &line[..at], &line[at..]));
            continue;
        }
        let mut brackets = HashMap::new();
        let mut bracket_pairs = Vec::new();
        let mut parens = HashMap::new();
        let mut bracket_stack = Vec::new();
        let mut paren_stack = Vec::new();
        let mut i = at;
        while i < bytes.len() {
            if line_mask.get(i) == Some(&b' ') {
                i += 1;
                continue;
            }
            if bytes[i] == b'{' {
                if let Some((end, _)) = read_djot_word_attributes(line, i) {
                    i = end;
                    continue;
                }
            }
            if bytes[i] == b'|' && i.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\') {
                bracket_stack.clear();
                paren_stack.clear();
                i += 1;
                continue;
            }
            if bytes[i] == b'\\' {
                let begin = i;
                while bytes.get(i) == Some(&b'\\') {
                    i += 1;
                }
                if (i - begin) % 2 != 0 && punctuation(bytes.get(i)) {
                    i += 1;
                }
                continue;
            }
            match bytes[i] {
                b'[' => bracket_stack.push(i),
                b']' => {
                    if let Some(open) = bracket_stack.pop() {
                        brackets.insert(open, i);
                        bracket_pairs.push((open, i));
                    }
                }
                b'(' => paren_stack.push(i),
                b')' => {
                    if let Some(open) = paren_stack.pop() {
                        parens.insert(open, i);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        let mut destinations = vec![0i32; bytes.len() + 1];
        let mut references: HashMap<usize, (usize, String)> = HashMap::new();
        for (open, close) in bracket_pairs {
            if bytes.get(open + 1) == Some(&b'^') {
                continue;
            }
            let next = close + 1;
            if bytes.get(next) == Some(&b'(') {
                if let Some(&last) = parens.get(&next) {
                    destinations[next + 1] += 1;
                    destinations[last] -= 1;
                }
            } else if bytes.get(next) == Some(&b'[') && !references.contains_key(&open) {
                let Some(&last) = brackets.get(&next) else {
                    continue;
                };
                let label = if last == next + 1 {
                    &line[open + 1..close]
                } else {
                    &line[next + 1..last]
                };
                let decoded = escapes.replace_all(label, "$1");
                let key = spaces.replace_all(&decoded, " ");
                let Some((target, attrs)) = definitions.get(key.trim()) else {
                    continue;
                };
                let target = target
                    .replace('\\', "%5C")
                    .replace('|', "%7C")
                    .replace('(', "%28")
                    .replace(')', "%29")
                    .replace('<', "%3C")
                    .replace('>', "%3E")
                    .replace(' ', "%20");
                let attrs = attribute_pipes_re.replace_all(attrs, |caps: &regex::Captures<'_>| {
                    let value = &caps[0];
                    if value.ends_with('|') && (value.len() - 1) % 2 == 0 {
                        format!("{}\\|", &value[..value.len() - 1])
                    } else {
                        value.to_owned()
                    }
                });
                references.insert(next, (last + 1, format!("({target}){attrs}")));
            }
        }
        let mut inside = 0;
        for count in &mut destinations {
            inside += *count;
            *count = inside;
        }
        let mut output = Vec::with_capacity(bytes.len());
        i = 0;
        while i < bytes.len() {
            if let Some((end, text)) = references.get(&i) {
                output.extend_from_slice(text.as_bytes());
                i = *end;
                continue;
            }
            if line_mask.get(i) == Some(&b' ') || bytes[i] != b'\\' {
                output.push(bytes[i]);
                i += 1;
                continue;
            }
            let begin = i;
            while bytes.get(i) == Some(&b'\\') {
                i += 1;
            }
            let run = i - begin;
            if bytes.get(i) != Some(&b'|') {
                output.extend_from_slice(&bytes[begin..i]);
                if run % 2 != 0 && punctuation(bytes.get(i)) {
                    output.push(bytes[i]);
                    i += 1;
                }
                continue;
            }
            if destinations[begin] > 0 {
                output.extend_from_slice("%5C".repeat(run / 2).as_bytes());
                output.extend_from_slice(b"%7C");
            } else {
                output.extend_from_slice("\\".repeat(run + usize::from(run % 2 == 0)).as_bytes());
                output.push(b'|');
            }
            i += 1;
        }
        result.push(String::from_utf8(output).expect("only ASCII syntax changed"));
    }
    result.join("\n")
}
