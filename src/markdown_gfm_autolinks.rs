use std::collections::HashMap;
use std::sync::OnceLock;

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};
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
    pub table: bool,
}

fn host_character(text: &str, at: usize) -> bool {
    static HOST: OnceLock<Regex> = OnceLock::new();
    text.get(at..).is_some_and(|tail| {
        HOST.get_or_init(|| Regex::new(r##"^[^\x09\x0a\x0c\x0d \u{00a0}\u{1680}\u{2000}-\u{200a}\u{202f}\u{205f}\u{3000}\p{P}!"-/:-@\[-`{-~]"##).unwrap())
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

fn markdown_destination_end(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let mut at = open + 1;
    let space = |at: &mut usize| {
        let start = *at;
        while bytes.get(*at).is_some_and(|byte| b" \t\r\n".contains(byte)) {
            *at += 1;
        }
        *at > start
    };
    space(&mut at);
    if bytes.get(at) == Some(&b'<') {
        at += 1;
        while at < bytes.len() && bytes[at] != b'>' {
            if bytes[at] == b'\\' {
                at += 2;
                continue;
            }
            if b"<\r\n".contains(&bytes[at]) {
                return None;
            }
            at += 1;
        }
        if bytes.get(at) != Some(&b'>') {
            return None;
        }
        at += 1;
    } else {
        let mut depth = 0usize;
        while at < bytes.len() {
            match bytes[at] {
                b'\\' => {
                    at += 2;
                    continue;
                }
                b'(' => {
                    depth += 1;
                    if depth > 32 {
                        return None;
                    }
                }
                b')' if depth == 0 => break,
                b')' => depth -= 1,
                byte if byte <= 0x20 || byte == 0x7f => {
                    if depth > 0 {
                        return None;
                    }
                    break;
                }
                _ => {}
            }
            at += 1;
        }
        if depth > 0 {
            return None;
        }
    }
    let spaced = space(&mut at);
    if bytes.get(at) == Some(&b')') {
        return Some(at);
    }
    let open = *bytes.get(at)?;
    if !spaced || !b"\"'(".contains(&open) {
        return None;
    }
    let close = if open == b'(' { b')' } else { open };
    at += 1;
    while at < bytes.len() && bytes[at] != close {
        if bytes[at] == b'\\' {
            at += 2;
            continue;
        }
        if close == b')' && bytes[at] == b'(' {
            return None;
        }
        at += 1;
    }
    if at >= bytes.len() {
        return None;
    }
    at += 1;
    space(&mut at);
    (bytes.get(at) == Some(&b')')).then_some(at)
}

fn raw_html_end(
    text: &str,
    at: usize,
    closers: &mut HashMap<&'static str, Option<usize>>,
) -> Option<usize> {
    let tail = &text[at..];
    for (open, close) in [("<!--", "-->"), ("<?", "?>"), ("<![CDATA[", "]]>")] {
        if !tail.starts_with(open) {
            continue;
        }
        let from = at + open.len();
        let end = closers
            .entry(close)
            .or_insert_with(|| text[from..].find(close).map(|offset| from + offset));
        if end.is_some_and(|end| end < from) {
            *end = text[from..].find(close).map(|offset| from + offset);
        }
        return end.map(|end| end + close.len());
    }
    if tail.starts_with("<!") && tail.as_bytes().get(2).is_some_and(u8::is_ascii_alphabetic) {
        let from = at + 2;
        let end = closers
            .entry(">")
            .or_insert_with(|| text[from..].find('>').map(|offset| from + offset));
        if end.is_some_and(|end| end < from) {
            *end = text[from..].find('>').map(|offset| from + offset);
        }
        return end.map(|end| end + 1);
    }
    static TAG: OnceLock<Regex> = OnceLock::new();
    let tag = TAG.get_or_init(|| Regex::new(r#"^</?[A-Za-z][A-Za-z0-9-]*(?:[ \t\r\n]+[A-Za-z_:][A-Za-z0-9_.:-]*(?:[ \t\r\n]*=[ \t\r\n]*(?:"[^"]*"|'[^']*'|[^ \t\r\n"'=<>`]+))?)*[ \t\r\n]*/?>"#).unwrap());
    if let Some(tag) = tag.find(tail) {
        return Some(at + tag.end());
    }
    static ANGLE: OnceLock<Regex> = OnceLock::new();
    let angle = ANGLE.get_or_init(|| Regex::new(r#"^<(?:[A-Za-z][A-Za-z0-9+.-]{1,31}:[^<>\s]*|[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9.-]+)>"#).unwrap());
    angle.find(tail).map(|matched| at + matched.end())
}

pub(crate) fn bare_links(text: &str, mode: Mode) -> Vec<BareLink> {
    scan_links(text, mode, false, &[])
}

fn scan_links(
    text: &str,
    mode: Mode,
    raw: bool,
    references: &[std::ops::Range<usize>],
) -> Vec<BareLink> {
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
    let mut html_closers = HashMap::new();
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
            if width > 1 {
                if let Some(close) = next.get(&(width - 1)) {
                    code_closers.insert(start + 1, close + width - 1);
                }
            }
            if let Some(close) = next.insert(width, start) {
                code_closers.insert(start, close + width);
            }
        }
    }
    let mut links: Vec<BareLink> = Vec::new();
    let (mut search, mut context, mut chunk_end, mut email_floor) = (0, 0, 0, 0);
    let mut brackets: Vec<(usize, bool)> = Vec::new();
    let mut generation = 0usize;
    let mut reference_at = 0;
    let mut escaped_bang = None;
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
            while references
                .get(reference_at)
                .is_some_and(|range| range.end <= context)
            {
                reference_at += 1;
            }
            if let Some(range) = references
                .get(reference_at)
                .filter(|range| range.start == context)
            {
                generation += 1;
                context = range.end;
                reference_at += 1;
                continue;
            }
            if bytes[context] == b'\\'
                && bytes.get(context + 1).is_some_and(u8::is_ascii_punctuation)
            {
                if bytes.get(context + 1) == Some(&b'!') {
                    escaped_bang = Some(context + 1);
                }
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
                if let Some(end) = raw_html_end(text, context, &mut html_closers) {
                    context = end;
                    continue;
                }
            }
            match bytes[context] {
                b'[' => {
                    let image = context > 0
                        && bytes[context - 1] == b'!'
                        && escaped_bang != Some(context - 1);
                    brackets.push((generation, image));
                }
                b']' => {
                    if let Some((opened, image)) = brackets.pop() {
                        let active = image || opened == generation;
                        if raw && active {
                            if let Some(end) = markdown_destination_end(text, context + 1) {
                                if !image {
                                    generation += 1;
                                }
                                context = end + 1;
                                continue;
                            }
                        }
                    }
                }
                _ => {}
            }
            context += 1;
        }
        if context > start_at {
            continue;
        }
        let email = trigger == b"@";
        let www = trigger == b"www.";
        if (!email && !brackets.is_empty()) || (www && &bytes[start..end] != b"www.") {
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
            let mut auto_mailto = carried.map_or(true, |item| item.3);
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
                && !(matches!(bytes[start - 1], b' ' | b'\t' | b'\n' | b'\r')
                    || b"*_~(".contains(&bytes[start - 1]))
            {
                continue;
            }
            if !www && start > 0 && bytes[start - 1].is_ascii_alphabetic() {
                continue;
            }
            if end >= chunk_end {
                chunk_end = end;
                while chunk_end < bytes.len()
                    && !matches!(bytes[chunk_end], b' ' | b'\t' | b'\n' | b'\r')
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
        links.push(BareLink {
            start,
            end,
            prefix,
            table: false,
        });
        context = end;
        search = end;
    }
    links
}

pub(crate) fn prepare_urls(
    source: &str,
    options: Options,
) -> Option<(String, Vec<BareLink>, bool)> {
    if !source.contains("://") && !source.contains("www.") {
        return None;
    }
    let mut references = Vec::new();
    let mut line_starts = vec![0];
    line_starts.extend(
        source
            .bytes()
            .enumerate()
            .filter_map(|(at, byte)| (byte == b'\n').then_some(at + 1)),
    );
    let mut line_content = HashMap::<usize, usize>::new();
    let mut ranges = Vec::new();
    let mut group: Option<std::ops::Range<usize>> = None;
    let mut opaque_block = 0usize;
    let (mut quote_depth, mut group_quotes, mut in_table_cell, mut group_table) =
        (0usize, 0usize, false, false);
    let mut inline_start = None;
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        if matches!(
            &event,
            Event::Start(Tag::Link {
                link_type: LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut,
                ..
            })
        ) {
            references.push(range.clone());
        }
        if matches!(
            &event,
            Event::Text(_)
                | Event::Code(_)
                | Event::InlineHtml(_)
                | Event::InlineMath(_)
                | Event::DisplayMath(_)
                | Event::Start(
                    Tag::Link { .. }
                        | Tag::Image { .. }
                        | Tag::Emphasis
                        | Tag::Strong
                        | Tag::Strikethrough
                )
        ) {
            let line = line_starts
                .partition_point(|at| *at <= range.start)
                .saturating_sub(1);
            line_content
                .entry(line_starts[line])
                .and_modify(|at| *at = (*at).min(range.start))
                .or_insert(range.start);
        }
        match &event {
            Event::Start(Tag::BlockQuote(_)) => quote_depth += 1,
            Event::End(TagEnd::BlockQuote(_)) => quote_depth = quote_depth.saturating_sub(1),
            Event::Start(Tag::TableCell) => in_table_cell = true,
            Event::End(TagEnd::TableCell) => in_table_cell = false,
            _ => {}
        }
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
                ranges.push((group, group_quotes, group_table));
            }
            inline_start = None;
            continue;
        }
        let inline = matches!(
            event,
            Event::Text(_)
                | Event::Code(_)
                | Event::InlineMath(_)
                | Event::DisplayMath(_)
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
                group_quotes = quote_depth;
                group_table = in_table_cell;
            }
        } else {
            if let Some(group) = group.take() {
                ranges.push((group, group_quotes, group_table));
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
        ranges.push((group, group_quotes, group_table));
    }
    let mut candidates = Vec::new();
    for (range, quotes, table) in ranges {
        let mut subject = source.as_bytes()[range.clone()].to_vec();
        if quotes > 0 {
            let mut line_start = 0;
            while line_start < subject.len() {
                let mut at = line_start;
                for _ in 0..quotes {
                    let source_at = range.start + line_start;
                    let line = line_starts
                        .partition_point(|start| *start <= source_at)
                        .saturating_sub(1);
                    let content_start = line_content
                        .get(&line_starts[line])
                        .copied()
                        .unwrap_or(source_at);
                    while range.start + at < content_start
                        && matches!(subject.get(at), Some(b' ' | b'\t'))
                    {
                        at += 1;
                    }
                    if range.start + at >= content_start || subject.get(at) != Some(&b'>') {
                        break;
                    }
                    subject[at] = b' ';
                    at += 1;
                    if matches!(subject.get(at), Some(b' ' | b'\t')) {
                        at += 1;
                    }
                }
                match subject[line_start..].iter().position(|byte| *byte == b'\n') {
                    Some(end) => line_start += end + 1,
                    None => break,
                }
            }
        }
        let subject = String::from_utf8(subject).expect("ASCII replacement preserves UTF-8");
        let first_reference = references.partition_point(|reference| reference.start < range.start);
        let last_reference = references.partition_point(|reference| reference.start < range.end);
        let group_references: Vec<_> = references[first_reference..last_reference]
            .iter()
            .filter(|reference| reference.end <= range.end)
            .map(|reference| reference.start - range.start..reference.end - range.start)
            .collect();
        for mut link in scan_links(&subject, Mode::Url, true, &group_references) {
            link.table = table;
            link.start += range.start;
            link.end += range.start;
            candidates.push(link);
        }
    }
    let mut links = Vec::new();
    let mut masked = source.as_bytes().to_vec();
    for link in candidates {
        for byte in &mut masked[link.start..link.end] {
            if b"*_~[]`\\&$".contains(byte) || (link.table && *byte == b'|') {
                *byte = b'\'';
            }
        }
        links.push(link);
    }
    if links.is_empty() {
        None
    } else {
        let masked = String::from_utf8(masked).expect("ASCII replacement preserves UTF-8");
        let mut claimed = 0;
        for (event, range) in Parser::new_ext(&masked, options).into_offset_iter() {
            if !matches!(event, Event::Text(_)) {
                continue;
            }
            while let Some(link) = links.get(claimed).filter(|link| link.end <= range.end) {
                if link.start < range.start {
                    break;
                }
                claimed += 1;
            }
        }
        if claimed != links.len() {
            Some((source.to_owned(), Vec::new(), false))
        } else {
            Some((masked, links, true))
        }
    }
}

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
struct Emails {
    link_depth: usize,
}
impl SubtreeVisitor for Emails {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        for block in blocks {
            visit_block_children(block, self);
        }
    }
    fn inlines(&mut self, nodes: &mut Vec<InlineNode>) {
        let mut source = std::mem::take(nodes).into_iter().peekable();
        while let Some(mut node) = source.next() {
            if let Some(first) = text(&node).filter(|_| self.link_depth == 0) {
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
                let inside_link = matches!(&node, InlineNode::Link(_) | InlineNode::Image(_))
                    || matches!(&node, InlineNode::Span(span) if span.injected && span.attrs.is_none());
                if inside_link {
                    self.link_depth += 1;
                }
                visit_inline_children(&mut node, self);
                if inside_link {
                    self.link_depth -= 1;
                }
                match node {
                    InlineNode::Span(span) if span.injected && span.attrs.is_none() => {
                        nodes.extend(span.children);
                    }
                    node => nodes.push(node),
                }
            }
        }
    }
}
pub(crate) fn link_email_text(document: &mut crate::ast::Document) {
    let mut visitor = Emails { link_depth: 0 };
    visitor.blocks(&mut document.children);
    for blocks in document.footnote_defs.values_mut() {
        visitor.blocks(blocks);
    }
}

pub(crate) fn link_email_inlines(nodes: &mut Vec<InlineNode>) {
    let mut visitor = Emails { link_depth: 0 };
    visitor.inlines(nodes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use html5ever::{parse_document, tendril::TendrilSink};
    use markup5ever_rcdom::{Handle, NodeData, RcDom};
    use serde_json::{json, Value};

    fn html_shape(html: &str) -> Value {
        fn normalize_text(text: &str, literal: bool) -> String {
            if literal {
                text.to_owned()
            } else {
                static SPACE: OnceLock<Regex> = OnceLock::new();
                SPACE
                    .get_or_init(|| Regex::new(r"[\t\r\n\x0c ]+").unwrap())
                    .replace_all(text, " ")
                    .into_owned()
            }
        }
        fn visit(node: &Handle, literal: bool) -> Option<Value> {
            match &node.data {
                NodeData::Text { contents } => {
                    let text = contents.borrow();
                    let value = normalize_text(&text, literal);
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
                    let mut joined: Vec<Value> = Vec::with_capacity(children.len());
                    for child in children {
                        if let Some(previous) = joined
                            .last_mut()
                            .filter(|previous| previous[0] == "text" && child[0] == "text")
                        {
                            previous[1] = json!(normalize_text(
                                &format!(
                                    "{}{}",
                                    previous[1].as_str().unwrap(),
                                    child[1].as_str().unwrap()
                                ),
                                literal
                            ));
                        } else {
                            joined.push(child);
                        }
                    }
                    children = joined;
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
    #[test]
    fn empty_destinations_keep_heading_and_table_normalization() {
        for source in ["# []()\n", "# ![]()\n"] {
            let document = crate::markdown_import::try_markdown_to_ast(source).unwrap();
            assert!(
                matches!(&document.children[0], BlockNode::RawBlock(raw) if raw.format == "html" && raw.content.contains("<h1>")),
                "{source:?}"
            );
        }
        let source = "| a |\n|---|\n| []() |\n";
        let result = crate::migrate_markdown(source);
        assert!(result
            .report
            .diagnostics
            .iter()
            .any(|row| row.message.contains("blank cell")));
        let document = crate::markdown_import::try_markdown_to_ast(source).unwrap();
        assert!(matches!(&document.children[0], BlockNode::Table(table) if table.rows.len() == 1));
    }

    #[test]
    fn empty_destinations_keep_email_labels_without_new_links() {
        for source in [
            "[a@b.cd]()\n",
            "![a@b.cd]()\n",
            "[![a@b.cd]()](u)\n",
            "[a ![a@b.cd]() b]()\n",
            "[a@b.cd](<> \"\")\n",
            "[a@b.cd](<> \"title\")\n",
            "![a@b.cd](<> \"title\")\n",
        ] {
            let result = crate::migrate_markdown(source);
            let html = crate::render_html(&crate::parse(&result.value)).unwrap();
            assert!(html.contains("a@b.cd"), "{source:?}: {html}");
            assert!(!html.contains("mailto:"), "{source:?}: {html}");
            assert!(!html.contains("<img"), "{source:?}: {html}");
            assert!(
                result
                    .report
                    .diagnostics
                    .iter()
                    .any(|row| { row.message.contains("empty destination") }),
                "{source:?}"
            );
        }
    }

    #[test]
    fn autolink_reports_follow_actual_context_and_merged_email_events() {
        for (source, links, escapes) in [
            ("a@b\\.test\n", 1, 1),
            ("a@b.cd http://x.y\n", 2, 0),
            ("http://a.b/\\< x\n", 1, 0),
            ("\\*http://a.b\n", 1, 1),
            ("- a\n  [ b\n  http://x.test\n", 0, 0),
            ("foo&amp;www.x.y\n", 0, 0),
        ] {
            let result = crate::migrate_markdown(source);
            let count = |code| {
                result
                    .report
                    .diagnostics
                    .iter()
                    .filter(|row| row.code == code)
                    .count()
            };
            assert_eq!(count("markdown-bare-autolink"), links, "{source:?}");
            assert_eq!(count("markdown-escape"), escapes, "{source:?}");
        }
        for source in ["```\na", "    a"] {
            let result = crate::migrate_markdown(source);
            assert!(
                !result
                    .report
                    .diagnostics
                    .iter()
                    .any(|row| row.code == "fidelity-unverified"),
                "{source:?}"
            );
        }
    }
}
