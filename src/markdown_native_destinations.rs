use std::collections::{HashMap, HashSet};

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};

struct Destination {
    end: usize,
    start: usize,
    stop: usize,
    angle: bool,
    needs_rewrite: bool,
}

struct Open {
    at: usize,
    nested_link: bool,
}

fn whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n')
}

fn inline_destination(bytes: &[u8], open: usize) -> Option<Destination> {
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let mut at = open + 1;
    while bytes.get(at).copied().is_some_and(whitespace) {
        at += 1;
    }
    let start = at;
    let angle = bytes.get(at) == Some(&b'<');
    let mut escaped_line = false;
    let mut unbalanced = false;
    if angle {
        at += 1;
        while at < bytes.len() && bytes[at] != b'>' {
            if bytes[at] == b'\\' && at + 1 < bytes.len() {
                escaped_line |= bytes[at + 1] == b'\n';
                at += 2;
                if escaped_line && bytes.get(at) == Some(&b'>') {
                    let mut after = at + 1;
                    while bytes
                        .get(after)
                        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
                    {
                        after += 1;
                    }
                    if !matches!(bytes.get(after), Some(b')' | b'"' | b'\'')) {
                        at += 1;
                    }
                }
            } else if matches!(bytes[at], b'<' | b'\n') {
                return None;
            } else {
                at += 1;
            }
        }
        if bytes.get(at) != Some(&b'>') {
            return None;
        }
        at += 1;
    } else {
        let mut depth = 0;
        while at < bytes.len() {
            match bytes[at] {
                b'\\' if at + 1 < bytes.len() && bytes[at + 1].is_ascii_punctuation() => at += 2,
                b'(' => {
                    depth += 1;
                    if depth > 32 {
                        return None;
                    }
                    at += 1;
                }
                b')' if depth > 0 => {
                    depth -= 1;
                    at += 1;
                }
                b')' => break,
                byte if byte <= b' ' || byte == 0x7f => break,
                _ => at += 1,
            }
        }
        unbalanced = depth > 0;
        if at == bytes.len() && depth > 0 {
            return None;
        }
    }
    let stop = at;
    let mut spaced = false;
    while bytes.get(at).copied().is_some_and(whitespace) {
        spaced = true;
        at += 1;
    }
    if bytes.get(at) == Some(&b')') {
        return Some(Destination {
            end: at,
            start,
            stop,
            angle,
            needs_rewrite: escaped_line || unbalanced,
        });
    }
    if !spaced || !matches!(bytes.get(at), Some(b'\'' | b'"' | b'(')) {
        return None;
    }
    let close = if bytes[at] == b'(' { b')' } else { bytes[at] };
    at += 1;
    while at < bytes.len() && bytes[at] != close {
        if bytes[at] == b'\\' && at + 1 < bytes.len() {
            at += 2;
        } else if close == b')' && bytes[at] == b'(' {
            return None;
        } else {
            at += 1;
        }
    }
    if bytes.get(at) != Some(&close) {
        return None;
    }
    at += 1;
    while bytes.get(at).copied().is_some_and(whitespace) {
        at += 1;
    }
    (bytes.get(at) == Some(&b')')).then_some(Destination {
        end: at,
        start,
        stop,
        angle,
        needs_rewrite: escaped_line || unbalanced,
    })
}

fn decoded_bare_destination(raw: &str) -> Option<String> {
    let wrapped = format!("[x](<{raw}>)");
    Parser::new(&wrapped).find_map(|event| match event {
        Event::Start(Tag::Link { dest_url, .. }) => Some(dest_url.to_string()),
        _ => None,
    })
}

fn container_depths(prefix: &str) -> (usize, usize) {
    let mut rest = prefix.trim_start_matches([' ', '\t']);
    let mut quotes = 0;
    let mut lists = 0;
    loop {
        let bytes = rest.as_bytes();
        if bytes.first() == Some(&b'>')
            && bytes
                .get(1)
                .map_or(true, |byte| matches!(byte, b' ' | b'\t'))
        {
            quotes += 1;
            rest = rest[1..].trim_start_matches([' ', '\t']);
            continue;
        }
        let marker = if matches!(bytes.first(), Some(b'-' | b'*' | b'+'))
            && bytes
                .get(1)
                .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
        {
            Some(1)
        } else {
            let width = bytes
                .iter()
                .take_while(|byte| byte.is_ascii_alphanumeric())
                .count();
            (width > 0
                && matches!(bytes.get(width), Some(b'.' | b')'))
                && bytes
                    .get(width + 1)
                    .is_some_and(|byte| matches!(byte, b' ' | b'\t')))
            .then_some(width + 1)
        };
        if let Some(width) = marker {
            lists += 1;
            rest = rest[width..].trim_start_matches([' ', '\t']);
        } else {
            break;
        }
    }
    (quotes, lists)
}

fn normalize_angle_destination(raw: &str, source: &str, opener: usize) -> String {
    let mut value = String::new();
    let bytes = raw.as_bytes();
    let line_start = source[..opener].rfind('\n').map_or(0, |at| at + 1);
    let prefix = &source[line_start..opener];
    let (quote_depth, list_depth) = container_depths(prefix);
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\\' && bytes.get(at + 1) == Some(&b'\n') {
            value.push('\\');
            value.push('\n');
            at += 2;
            if bytes.get(at) == Some(&b'>') {
                at += 1;
                let strip = if quote_depth > 1 { 1 } else { usize::MAX };
                let mut skipped = 0;
                while bytes
                    .get(at)
                    .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
                    && skipped < strip
                {
                    at += 1;
                    skipped += 1;
                }
            } else if quote_depth == 0 || list_depth > 0 {
                let strip = if list_depth > 1 { 2 } else { usize::MAX };
                let mut skipped = 0;
                while bytes
                    .get(at)
                    .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
                    && skipped < strip
                {
                    at += 1;
                    skipped += 1;
                }
            }
            continue;
        }
        let ch = raw[at..].chars().next().unwrap();
        value.push(ch);
        at += ch.len_utf8();
    }
    value
}

fn reference_key(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn escaped(bytes: &[u8], at: usize) -> bool {
    let mut before = at;
    while before > 0 && bytes[before - 1] == b'\\' {
        before -= 1;
    }
    (at - before) % 2 != 0
}

fn opaque_ranges(parser: Parser<'_>) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut block_start = None;
    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock) => block_start = Some(range.start),
            Event::End(TagEnd::CodeBlock | TagEnd::HtmlBlock) => {
                if let Some(start) = block_start.take() {
                    ranges.push(start..range.end);
                }
            }
            Event::Code(_) | Event::InlineHtml(_) | Event::Html(_) => ranges.push(range),
            _ => {}
        }
    }
    ranges.sort_by_key(|range| range.start);
    ranges
}

pub(super) fn prepare(source: &str, options: Options) -> (String, HashMap<usize, String>) {
    if !source.contains("](") {
        return (source.to_owned(), HashMap::new());
    }
    let bytes = source.as_bytes();
    let mut output = bytes.to_vec();
    let mut links = HashMap::new();
    let mut rewritten = Vec::new();
    let parser = Parser::new_ext(source, options);
    let references: HashSet<String> = parser
        .reference_definitions()
        .iter()
        .map(|(key, _)| reference_key(key))
        .collect();
    let opaque = opaque_ranges(parser);
    let mut opaque_at = 0;
    let mut opens: Vec<Open> = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        while opaque.get(opaque_at).is_some_and(|range| range.end <= at) {
            opaque_at += 1;
        }
        if let Some(range) = opaque.get(opaque_at).filter(|range| range.start <= at) {
            at = range.end;
            continue;
        }
        if bytes[at] == b'\n'
            && source[at + 1..]
                .trim_start_matches([' ', '\t'])
                .starts_with('\n')
        {
            opens.clear();
        }
        if bytes[at] == b'\\' && at + 1 < bytes.len() {
            at += 2;
            continue;
        }
        if bytes[at] == b'<'
            && bytes.get(at + 1).is_some_and(|byte| {
                byte.is_ascii_alphabetic() || matches!(byte, b'!' | b'/' | b'?')
            })
        {
            if let Some(end) = bytes[at..]
                .iter()
                .take_while(|byte| **byte != b'\n')
                .position(|byte| *byte == b'>')
            {
                at += end + 1;
                continue;
            }
        }
        if bytes[at] == b'[' {
            opens.push(Open {
                at,
                nested_link: false,
            });
            at += 1;
            continue;
        }
        if bytes[at] == b']' {
            if let Some(open) = opens.pop() {
                let image =
                    open.at > 0 && bytes[open.at - 1] == b'!' && !escaped(bytes, open.at - 1);
                if let Some(dest) = inline_destination(bytes, at + 1) {
                    if dest.needs_rewrite && (image || !open.nested_link) {
                        let raw = &source[dest.start..dest.stop];
                        let decoded = if dest.angle {
                            Some(normalize_angle_destination(
                                &raw[1..raw.len() - 1],
                                source,
                                open.at,
                            ))
                        } else {
                            decoded_bare_destination(raw)
                        };
                        if let Some(decoded) = decoded {
                            output[dest.start..dest.stop].fill(b'u');
                            let key = if image { open.at - 1 } else { open.at };
                            links.insert(key, decoded);
                            rewritten.push((key, dest.start, dest.stop));
                        }
                    }
                    if !image && !open.nested_link {
                        for parent in &mut opens {
                            parent.nested_link = true;
                        }
                    }
                    at = dest.end + 1;
                    continue;
                }
                let explicit = bytes.get(at + 1) == Some(&b'[');
                let reference = if explicit {
                    bytes[at + 2..]
                        .iter()
                        .position(|byte| *byte == b']')
                        .and_then(|width| source.get(at + 2..at + 2 + width))
                        .filter(|label| !label.is_empty())
                        .unwrap_or(&source[open.at + 1..at])
                } else {
                    &source[open.at + 1..at]
                };
                if !image && !open.nested_link && references.contains(&reference_key(reference)) {
                    for parent in &mut opens {
                        parent.nested_link = true;
                    }
                    if explicit {
                        if let Some(width) = bytes[at + 2..].iter().position(|byte| *byte == b']') {
                            at += width + 3;
                            continue;
                        }
                    }
                }
            }
        }
        at += 1;
    }
    if !rewritten.is_empty() {
        let candidate = std::str::from_utf8(&output).expect("replacement keeps UTF-8");
        let active: HashSet<usize> = Parser::new_ext(candidate, options)
            .into_offset_iter()
            .filter_map(|(event, range)| {
                matches!(
                    event,
                    Event::Start(
                        Tag::Link {
                            link_type: LinkType::Inline,
                            ..
                        } | Tag::Image {
                            link_type: LinkType::Inline,
                            ..
                        }
                    )
                )
                .then_some(range.start)
            })
            .collect();
        for (key, start, stop) in rewritten {
            if !active.contains(&key) {
                output[start..stop].copy_from_slice(&bytes[start..stop]);
                links.remove(&key);
            }
        }
    }
    (
        String::from_utf8(output).expect("replacement keeps UTF-8"),
        links,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn rewrites_unbalanced_destination_without_moving_offsets() {
        let (source, urls) = super::prepare("[a](/x( \"t\")", pulldown_cmark::Options::empty());
        assert_eq!(source, "[a](uuu \"t\")");
        assert_eq!(urls.get(&0).map(String::as_str), Some("/x("));
        assert_eq!(
            crate::markdown_to_carve("[a](/x( \"t\")"),
            "[a](/x\\( \"t\")\n"
        );
        let (source, urls) = super::prepare("> [a](<a\\\n> b>)", pulldown_cmark::Options::empty());
        assert_eq!(source.len(), "> [a](<a\\\n> b>)".len());
        assert_eq!(urls.get(&2).map(String::as_str), Some("a\\\nb"));
    }

    #[test]
    fn inactive_destinations_keep_their_authored_text() {
        for source in [
            "[a\n\nb](/x( \"t\")",
            "[g [p [c](/c) q](/p) r](/x( \"t\")",
            "[o ![i [l](/y)](/z) p](/x( \"t\")",
            "[o [a  b] p](/x( \"t\")\n\n[a b]: /a",
        ] {
            let written = crate::markdown_to_carve(source);
            assert!(!written.contains("uuu"), "{source:?}: {written}");
            assert!(
                crate::to_html(&written).contains("/x("),
                "{source:?}: {written}"
            );
        }
        let written = crate::markdown_to_carve("\\![a](/x( \"t\")");
        assert!(!written.contains("uuu"));
        assert!(crate::to_html(&written).contains("href=\"/x(\""));
    }

    #[test]
    fn inline_text_does_not_count_as_a_container_prefix() {
        for source in ["a > b [a](<a\\\n   b>)", "a - b - [a](<a\\\n   b>)"] {
            let written = crate::markdown_to_carve(source);
            assert!(
                crate::to_html(&written).contains("href=\"a\\%0Ab\""),
                "{source:?}: {written}"
            );
        }
    }
}
