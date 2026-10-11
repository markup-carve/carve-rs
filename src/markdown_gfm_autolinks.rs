use std::collections::HashMap;
use std::sync::OnceLock;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use regex::Regex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    All,
    Url,
    Email,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BareLink {
    pub start: usize,
    pub end: usize,
    pub prefix: &'static str,
}

fn host_character(text: &str, at: usize) -> bool {
    static HOST: OnceLock<Regex> = OnceLock::new();
    text.get(at..).is_some_and(|tail| {
        HOST.get_or_init(|| Regex::new(r##"^[^\s\p{P}!"-/:-@\[-`{-~]"##).unwrap())
            .is_match(tail)
    })
}

fn domain_allowed(text: &str, start: usize, short: bool) -> bool {
    let bytes = text.as_bytes();
    if start >= bytes.len() || !host_character(text, start) {
        return false;
    }
    if !bytes[start].is_ascii() {
        return short;
    }
    let (mut dots, mut previous, mut underscores) = (0, 0, 0);
    let mut at = start + 1;
    while at + 1 < bytes.len() {
        if bytes[at] == b'\\' && at + 2 < bytes.len() {
            at += 1;
        }
        match bytes[at] {
            byte if !byte.is_ascii() => break,
            b'_' => underscores += 1,
            b'.' => {
                dots += 1;
                previous = underscores;
                underscores = 0;
            }
            b'-' => {}
            _ if !host_character(text, at) => break,
            _ => {}
        }
        at += 1;
    }
    (short || dots > 0) && (dots > 10 || previous + underscores == 0)
}

fn trimmed_end(bytes: &[u8], start: usize, mut end: usize) -> usize {
    let opening = bytes[start..end]
        .iter()
        .filter(|byte| **byte == b'(')
        .count();
    let mut closing = bytes[start..end]
        .iter()
        .filter(|byte| **byte == b')')
        .count();
    while end > start {
        match bytes[end - 1] {
            b')' if closing > opening => {
                closing -= 1;
                end -= 1;
            }
            b'?' | b'!' | b'.' | b',' | b':' | b'*' | b'_' | b'~' | b'\'' | b'"' => end -= 1,
            b';' => {
                let mut entity = end.saturating_sub(2);
                while entity > start && bytes[entity].is_ascii_alphabetic() {
                    entity -= 1;
                }
                end = if entity + 2 < end && bytes[entity] == b'&' {
                    entity
                } else {
                    end - 1
                };
            }
            _ => break,
        }
    }
    end
}

pub(crate) fn bare_links(text: &str, mode: Mode) -> Vec<BareLink> {
    scan_links(text, mode, false)
}

fn scan_links(text: &str, mode: Mode, raw: bool) -> Vec<BareLink> {
    if (mode == Mode::Email && !text.contains('@'))
        || (mode == Mode::Url && !text.contains("://") && !text.contains("www."))
        || (mode == Mode::All
            && !text.contains('@')
            && !text.contains("://")
            && !text.contains("www."))
    {
        return Vec::new();
    }
    if mode == Mode::All {
        let mut result = Vec::new();
        let mut cursor = 0;
        for url in bare_links(text, Mode::Url) {
            for mut email in bare_links(&text[cursor..url.start], Mode::Email) {
                email.start += cursor;
                email.end += cursor;
                result.push(email);
            }
            cursor = url.end;
            result.push(url);
        }
        for mut email in bare_links(&text[cursor..], Mode::Email) {
            email.start += cursor;
            email.end += cursor;
            result.push(email);
        }
        return result;
    }
    let bytes = text.as_bytes();
    let mut code_closers = HashMap::new();
    let mut destinations = HashMap::new();
    if raw {
        let mut runs = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at] != b'`' {
                at += 1;
                continue;
            }
            let start = at;
            while bytes.get(at) == Some(&b'`') {
                at += 1;
            }
            runs.push((start, at - start));
        }
        let mut next = HashMap::new();
        for (start, width) in runs.into_iter().rev() {
            if let Some(close) = next.insert(width, start) {
                code_closers.insert(start, close + width);
            }
        }
        let mut stops = vec![Some(bytes.len()); bytes.len() + 1];
        for at in (0..bytes.len()).rev() {
            stops[at] = match bytes[at] {
                b'\\' if bytes.get(at + 1).is_some_and(|b| b"\\()".contains(b)) => {
                    stops.get(at + 2).copied().flatten()
                }
                b'(' => stops[at + 1].and_then(|close| {
                    (bytes.get(close) == Some(&b')'))
                        .then(|| stops[close + 1])
                        .flatten()
                }),
                b')' => Some(at),
                byte if byte.is_ascii_whitespace() => Some(at),
                _ => stops[at + 1],
            };
        }
        static REST: OnceLock<Regex> = OnceLock::new();
        let rest = REST.get_or_init(|| {
            Regex::new(r#"^(?: "(?:[^"\\]|\\[\s\S])*"| '(?:[^'\\]|\\[\s\S])*')?\)"#).unwrap()
        });
        let mut tails = HashMap::new();
        for at in 0..bytes.len() {
            if bytes[at] != b'(' {
                continue;
            }
            if let Some(stop) = stops[at + 1].filter(|stop| *stop >= at + 1) {
                let width = *tails.entry(stop).or_insert_with(|| {
                    text.get(stop..)
                        .and_then(|tail| rest.find(tail))
                        .map_or(0, |matched| matched.end())
                });
                if width > 0 {
                    destinations.insert(at, stop + width);
                }
            }
        }
    }
    let mut links: Vec<BareLink> = Vec::new();
    let (mut search, mut context, mut brackets, mut chunk_end, mut email_floor) =
        (0, 0, 0usize, 0, 0);
    let mut retry: Option<(usize, usize, bool, bool)> = None;
    while search < bytes.len() {
        let start_at = search;
        let trigger = [b"https://".as_slice(), b"http://", b"ftp://", b"www.", b"@"]
            .into_iter()
            .find(|trigger| {
                bytes
                    .get(search..search + trigger.len())
                    .is_some_and(|part| part.eq_ignore_ascii_case(trigger))
            });
        let Some(trigger) = trigger else {
            search += 1;
            continue;
        };
        search += trigger.len();
        let (mut start, mut end) = (start_at, search);
        while context < start_at {
            if bytes[context] == b'\\'
                && bytes.get(context + 1).is_some_and(u8::is_ascii_punctuation)
            {
                context += 2;
                continue;
            }
            if raw && bytes[context] == b'`' {
                if let Some(end) = code_closers.get(&context) {
                    context = *end;
                } else {
                    while bytes.get(context) == Some(&b'`') {
                        context += 1;
                    }
                }
                continue;
            }
            if raw && bytes[context] == b'<' {
                static OPAQUE: OnceLock<Regex> = OnceLock::new();
                let opaque = OPAQUE.get_or_init(|| Regex::new(r#"^(?:<!--[\s\S]*?-->|<(?:[A-Za-z][A-Za-z0-9+.-]{1,31}:[^<>\s]*|[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9.-]+)>|</?[A-Za-z][A-Za-z0-9-]*(?:[\s/][^<>]*)?>)"#).unwrap());
                if let Some(found) = opaque.find(&text[context..]) {
                    context += found.end();
                    continue;
                }
            }
            if raw && bytes[context] == b']' && brackets > 0 {
                if let Some(end) = destinations.get(&(context + 1)) {
                    brackets -= 1;
                    context = *end;
                    continue;
                }
            }
            match bytes[context] {
                b'[' => brackets += 1,
                b']' => brackets = brackets.saturating_sub(1),
                _ => {}
            }
            context += 1;
        }
        if context > start_at {
            continue;
        }
        let email = trigger == b"@";
        let www = trigger == b"www.";
        if (!email && brackets > 0) || (www && &bytes[start..end] != b"www.") {
            continue;
        }
        let mut prefix = "";
        if email {
            if mode == Mode::Url {
                continue;
            }
            let carried = retry.take().filter(|item| item.0 == start_at);
            let mut dots = carried.map_or(0, |item| item.1);
            let mut resource = carried.is_some_and(|item| item.2);
            let mut auto_mailto = carried.is_none_or(|item| item.3);
            let floor = email_floor.max(links.last().map_or(0, |link| link.end));
            while start > floor {
                let previous = bytes[start - 1];
                if previous.is_ascii_alphanumeric() || b".+_-".contains(&previous) {
                    start -= 1;
                    continue;
                }
                if previous == b':' {
                    let scheme = [b"mailto:".as_slice(), b"xmpp:"]
                        .into_iter()
                        .find(|scheme| {
                            start.checked_sub(scheme.len()).is_some_and(|before| {
                                before >= floor
                                    && &bytes[before..start] == *scheme
                                    && (before == 0 || !bytes[before - 1].is_ascii_alphanumeric())
                            })
                        });
                    if let Some(scheme) = scheme {
                        start -= 1;
                        auto_mailto = false;
                        resource |= scheme == b"xmpp:";
                        continue;
                    }
                }
                break;
            }
            if start == start_at {
                continue;
            }
            while end < bytes.len() {
                let byte = bytes[end];
                if byte == b'@' {
                    retry = Some((end, dots, resource, auto_mailto));
                    break;
                }
                if byte.is_ascii_alphanumeric()
                    || b"_-".contains(&byte)
                    || (resource && byte == b'/')
                {
                    end += 1;
                } else if byte == b'.' && bytes.get(end + 1).is_some_and(u8::is_ascii_alphanumeric)
                {
                    dots += 1;
                    end += 1;
                } else {
                    break;
                }
            }
            if retry.is_some_and(|item| item.0 == end) {
                search = end;
                continue;
            }
            if dots == 0 || !bytes[end - 1].is_ascii_alphabetic() {
                email_floor = end;
                search = end;
                continue;
            }
            if auto_mailto {
                prefix = "mailto:";
            }
        } else {
            if mode == Mode::Email {
                continue;
            }
            if www
                && start > 0
                && !(bytes[start - 1].is_ascii_whitespace() || b"*_~(".contains(&bytes[start - 1]))
            {
                continue;
            }
            if !www && start > 0 && bytes[start - 1].is_ascii_alphabetic() {
                continue;
            }
            if end >= chunk_end {
                chunk_end = end;
                while chunk_end < bytes.len()
                    && !bytes[chunk_end].is_ascii_whitespace()
                    && !b"<\0".contains(&bytes[chunk_end])
                {
                    chunk_end += 1;
                }
            }
            if !domain_allowed(text, if www { start } else { search }, !www) {
                continue;
            }
            end = trimmed_end(bytes, start, chunk_end);
            if www {
                prefix = "http://";
            }
        }
        links.push(BareLink { start, end, prefix });
        context = end;
        search = end;
    }
    links
}

pub(crate) fn prepare_urls(source: &str, options: Options) -> Option<(String, Vec<BareLink>)> {
    if !source.contains("://") && !source.contains("www.") {
        return None;
    }
    let mut ranges = Vec::new();
    let mut group: Option<std::ops::Range<usize>> = None;
    let mut opaque_block = 0usize;
    let mut inline_start = None;
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        match &event {
            Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::MetadataBlock(_)) => {
                opaque_block += 1
            }
            Event::End(TagEnd::CodeBlock | TagEnd::HtmlBlock | TagEnd::MetadataBlock(_)) => {
                opaque_block = opaque_block.saturating_sub(1);
                continue;
            }
            _ => {}
        }
        if opaque_block > 0 {
            if let Some(group) = group.take() {
                ranges.push(group);
            }
            inline_start = None;
            continue;
        }
        let inline = matches!(
            event,
            Event::Text(_)
                | Event::Code(_)
                | Event::InlineHtml(_)
                | Event::SoftBreak
                | Event::HardBreak
                | Event::FootnoteReference(_)
                | Event::Start(
                    Tag::Link { .. }
                        | Tag::Image { .. }
                        | Tag::Emphasis
                        | Tag::Strong
                        | Tag::Strikethrough
                )
                | Event::End(
                    TagEnd::Link
                        | TagEnd::Image
                        | TagEnd::Emphasis
                        | TagEnd::Strong
                        | TagEnd::Strikethrough
                )
        );
        if inline {
            if let Some(group) = &mut group {
                group.start = group.start.min(range.start);
                group.end = group.end.max(range.end);
            } else {
                group = Some(inline_start.unwrap_or(range.start)..range.end);
            }
        } else {
            if let Some(group) = group.take() {
                ranges.push(group);
            }
            inline_start = match event {
                Event::Start(Tag::Paragraph | Tag::Heading { .. } | Tag::TableCell | Tag::Item) => {
                    Some(range.start)
                }
                _ => None,
            };
        }
    }
    if let Some(group) = group {
        ranges.push(group);
    }
    let mut candidates = Vec::new();
    for range in ranges {
        for mut link in scan_links(&source[range.clone()], Mode::Url, true) {
            link.start += range.start;
            link.end += range.start;
            candidates.push(link);
        }
    }
    let mut links = Vec::new();
    let mut masked = source.as_bytes().to_vec();
    for link in candidates {
        for byte in &mut masked[link.start..link.end] {
            if b"*_~[]`\\&".contains(byte) {
                *byte = b'x';
            }
        }
        links.push(link);
    }
    if links.is_empty() {
        None
    } else {
        Some((
            String::from_utf8(masked).expect("ASCII replacement preserves UTF-8"),
            links,
        ))
    }
}

pub(crate) fn link_email_text(document: &mut crate::ast::Document) {
    use crate::ast::{BlockNode, InlineNode};
    use crate::include_walk::{visit_block_children, visit_inline_children, SubtreeVisitor};
    fn text(node: &InlineNode) -> Option<&str> {
        match node {
            InlineNode::Text(node) => Some(&node.value),
            InlineNode::EscapedText(node) => Some(&node.value),
            InlineNode::SmartPunctuation(node) => Some(&node.value),
            _ => None,
        }
    }
    struct Emails;
    impl SubtreeVisitor for Emails {
        fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
            for block in blocks {
                visit_block_children(block, self);
            }
        }
        fn inlines(&mut self, nodes: &mut Vec<InlineNode>) {
            let mut source = std::mem::take(nodes).into_iter().peekable();
            while let Some(mut node) = source.next() {
                if let Some(first) = text(&node) {
                    let mut value = first.to_owned();
                    let mut original = vec![node];
                    while let Some(next) = source.peek().and_then(text) {
                        value.push_str(next);
                        original.push(source.next().unwrap());
                    }
                    let links = bare_links(&value, Mode::Email);
                    if links.is_empty() {
                        nodes.extend(original);
                        continue;
                    }
                    let mut cursor = 0;
                    for link in links {
                        if cursor < link.start {
                            nodes.push(InlineNode::text(&value[cursor..link.start]));
                        }
                        nodes.push(crate::markdown_import::literal_bare_link(
                            &value[link.start..link.end],
                            link.prefix,
                        ));
                        cursor = link.end;
                    }
                    if cursor < value.len() {
                        nodes.push(InlineNode::text(&value[cursor..]));
                    }
                } else {
                    if !matches!(&node, InlineNode::Link(_) | InlineNode::Image(_)) {
                        visit_inline_children(&mut node, self);
                    }
                    nodes.push(node);
                }
            }
        }
    }
    let mut visitor = Emails;
    visitor.blocks(&mut document.children);
    for blocks in document.footnote_defs.values_mut() {
        visitor.blocks(blocks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use html5ever::{parse_document, tendril::TendrilSink};
    use markup5ever_rcdom::{Handle, NodeData, RcDom};
    use serde_json::{json, Value};

    fn html_shape(html: &str) -> Value {
        fn visit(node: &Handle, literal: bool) -> Option<Value> {
            match &node.data {
                NodeData::Text { contents } => {
                    let text = contents.borrow();
                    let value = if literal {
                        text.to_string()
                    } else {
                        static SPACE: OnceLock<Regex> = OnceLock::new();
                        SPACE
                            .get_or_init(|| Regex::new(r"[\t\r\n\x0c ]+").unwrap())
                            .replace_all(&text, " ")
                            .into_owned()
                    };
                    Some(json!(["text", value]))
                }
                NodeData::Element { name, attrs, .. } => {
                    let tag = match name.local.as_ref() {
                        "del" => "s",
                        "i" => "em",
                        other => other,
                    };
                    let literal = literal || matches!(tag, "code" | "pre");
                    let mut attributes = serde_json::Map::new();
                    for attr in attrs.borrow().iter() {
                        if tag == "th"
                            && attr.name.local.as_ref() == "scope"
                            && attr.value.as_ref() == "col"
                        {
                            continue;
                        }
                        let value = if matches!(attr.name.local.as_ref(), "checked" | "disabled") {
                            Value::Bool(true)
                        } else {
                            Value::String(attr.value.to_string())
                        };
                        attributes.insert(attr.name.local.to_string(), value);
                    }
                    let container = matches!(
                        tag,
                        "body"
                            | "blockquote"
                            | "ul"
                            | "ol"
                            | "dl"
                            | "table"
                            | "thead"
                            | "tbody"
                            | "tfoot"
                            | "tr"
                    );
                    let mut children: Vec<Value> = node
                        .children
                        .borrow()
                        .iter()
                        .filter_map(|child| visit(child, literal))
                        .filter(|child| {
                            !(container
                                && child[0] == "text"
                                && child[1].as_str().is_some_and(|text| {
                                    text.trim_matches([' ', '\t', '\n', '\r', '\x0c'])
                                        .is_empty()
                                }))
                        })
                        .collect();
                    if !literal && matches!(tag, "p" | "li" | "dt" | "dd" | "blockquote") {
                        if let Some(first) = children.first_mut().filter(|child| child[0] == "text")
                        {
                            first[1] = json!(first[1].as_str().unwrap().trim_start_matches(' '));
                        }
                        if let Some(last) = children.last_mut().filter(|child| child[0] == "text") {
                            last[1] = json!(last[1].as_str().unwrap().trim_end_matches(' '));
                        }
                    }
                    if tag == "p" && children.len() == 1 && children[0][0] == "img" {
                        return children.pop();
                    }
                    Some(json!([tag, attributes, children]))
                }
                _ => None,
            }
        }
        let dom = parse_document(RcDom::default(), Default::default()).one(html);
        let mut pending = vec![dom.document.clone()];
        while let Some(node) = pending.pop() {
            if matches!(&node.data, NodeData::Element { name, .. } if name.local.as_ref() == "body")
            {
                return visit(&node, false).unwrap();
            }
            pending.extend(node.children.borrow().iter().cloned());
        }
        panic!("HTML parser did not create a body");
    }

    #[test]
    fn native_gfm_controls_survive_import_render_and_source_formatting() {
        let rows: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/markdown-gfm-autolinks.json"
        ))
        .unwrap();
        for row in rows.as_array().unwrap() {
            let source = row["source"].as_str().unwrap();
            let expected = html_shape(row["html"].as_str().unwrap());
            let document = crate::markdown_import::try_markdown_to_ast(source).unwrap();
            let written = crate::markdown_import::try_markdown_to_carve(source).unwrap();
            let parsed = crate::parse(&written);
            let formatted = crate::render_carve(&parsed).unwrap();
            for actual in [
                crate::render_html(&document).unwrap(),
                crate::render_html(&parsed).unwrap(),
                crate::render_html(&crate::parse(&formatted)).unwrap(),
            ] {
                assert_eq!(expected, html_shape(&actual), "{source:?}");
            }
        }
    }
}
