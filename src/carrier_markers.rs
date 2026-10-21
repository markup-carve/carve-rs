//! The carrier marker PART 11 §10s defines: an HTML comment holding the Carve
//! opener or closer of a container the Markdown target writes as its children
//! alone (CARVE-P11-063).
//!
//! The payload is Carve source, so Carve's own escape is the one the reader
//! already has and only the comment's own terminator is rewritten.

use crate::ast::{BlockNode, Document, OrderedListType};

pub(crate) const CARRIER_PREFIX: &str = "<!-- carve: ";
const CARRIER_STEM: &str = "<!-- carve:";
pub(crate) const CARRIER_SUFFIX: &str = " -->";

/// A `-->` the payload carries becomes `--\>`; a backslash run already sitting
/// where that escape would put one grows by one, so the transform reverses
/// exactly.
pub(crate) fn escape_carrier_payload(payload: &str) -> String {
    rewrite_terminator(payload, false)
}

pub(crate) fn unescape_carrier_payload(payload: &str) -> String {
    rewrite_terminator(payload, true)
}

/// Walk `--` followed by a backslash run and a `>`, adding one backslash to the
/// run (`escaped`) or removing one (`unescape`).
fn rewrite_terminator(payload: &str, unescape: bool) -> String {
    let bytes = payload.as_bytes();
    let mut out = String::with_capacity(payload.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'-' || bytes.get(at + 1) != Some(&b'-') {
            let ch = payload[at..].chars().next().unwrap();
            out.push(ch);
            at += ch.len_utf8();
            continue;
        }
        let mut end = at + 2;
        while bytes.get(end) == Some(&b'\\') {
            end += 1;
        }
        let slashes = end - (at + 2);
        if bytes.get(end) != Some(&b'>') || (unescape && slashes == 0) {
            out.push_str("--");
            at += 2;
            continue;
        }
        out.push_str("--");
        let kept = if unescape { slashes - 1 } else { slashes + 1 };
        for _ in 0..kept {
            out.push('\\');
        }
        out.push('>');
        at = end + 1;
    }

    out
}

/// Write a payload into a marker line.
pub(crate) fn carrier_line(payload: &str) -> String {
    carrier_line_at(payload, 1)
}

/// Write a definition-list marker at its nesting depth.
pub(crate) fn carrier_line_at(payload: &str, depth: usize) -> String {
    let prefix = if depth == 1 {
        CARRIER_PREFIX.to_owned()
    } else {
        format!("{CARRIER_STEM}{depth} ")
    };
    if payload.is_empty() {
        return format!("{prefix}-->");
    }
    format!(
        "{prefix}{}{CARRIER_SUFFIX}",
        escape_carrier_payload(payload)
    )
}

pub(crate) fn code_fence_marker(code: &crate::ast::CodeBlock) -> Option<String> {
    if code.title.is_none() && code.label.as_deref().map_or(true, str::is_empty) {
        return None;
    }
    let mut bare = code.clone();
    bare.content.clear();
    bare.attrs = None;
    let mut doc = crate::parse::parse("");
    doc.children.push(BlockNode::CodeBlock(bare));
    let source = crate::render_carve::render_carve(&doc).ok()?;
    source.lines().next().map(carrier_line)
}

/// The payload a marker line carries, or `None` when the line is not one.
pub(crate) fn carrier_payload(line: &str) -> Option<String> {
    carrier_payload_at(line).map(|(payload, _)| payload)
}

fn carrier_payload_at(line: &str) -> Option<(String, usize)> {
    fn payload(tail: &str) -> Option<&str> {
        if tail == " -->" {
            Some("")
        } else {
            tail.strip_prefix(' ')?.strip_suffix(CARRIER_SUFFIX)
        }
    }

    let rest = line.strip_prefix(CARRIER_STEM)?;
    let (depth, inner) = if rest.starts_with(' ') {
        (1, payload(rest)?)
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let depth = rest[..digits]
            .parse::<usize>()
            .ok()
            .filter(|depth| *depth >= 2)?;
        let inner = payload(&rest[digits..])?;
        (depth, inner)
    };
    // An unescaped `-->` inside would have ended the comment at that point, so
    // the line this engine is reading is not one this engine wrote.
    if inner.contains("-->") {
        return None;
    }

    Some((unescape_carrier_payload(inner), depth))
}

/// The colon-fence width a payload opens or closes with, or 0 when the payload
/// is neither (an attribute line travelling with its opener).
pub(crate) fn carrier_fence_width(payload: &str) -> usize {
    let colons = payload.bytes().take_while(|byte| *byte == b':').count();
    if colons >= 3 {
        colons
    } else {
        0
    }
}

/// Whether a payload is a bare colon fence; its role depends on the open set.
pub(crate) fn carrier_is_bare_fence(payload: &str) -> bool {
    carrier_fence_width(payload) == payload.len()
}

fn code_fence_marker_info(payload: &str) -> Option<&str> {
    let delimiter = *payload.as_bytes().first()?;
    if delimiter != b'`' && delimiter != b'~' {
        return None;
    }
    let run = payload
        .bytes()
        .take_while(|byte| *byte == delimiter)
        .count();
    if run < 3 || payload.as_bytes().get(run - 1) != payload.as_bytes().first() {
        return None;
    }
    let rest = payload[run..].trim_start();
    let fence = &payload[..run];
    let parsed = crate::parse::parse(&format!("{payload}\n{fence}\n"));
    let [BlockNode::CodeBlock(code)] = parsed.children.as_slice() else {
        return None;
    };
    if code.title.is_none() && code.label.is_none() {
        return None;
    }
    let info = match code.lang.as_deref() {
        Some(lang) => rest.strip_prefix(lang)?.trim_start(),
        None => rest,
    };
    (!info.is_empty()).then_some(info)
}

/// The Carve lines a container's carrier markers carry.
pub(crate) struct CarrierMarkers {
    /// The attribute line travelling above the opener, if the container has one.
    pub(crate) prelude: Vec<String>,
    pub(crate) opener: String,
    pub(crate) closer: String,
}

/// The markers a container takes, or `None` when the node has no colon-fence
/// spelling.
///
/// The payload is spelled by the CANONICAL WRITER rather than composed a second
/// time here: it is Carve source, and a second spelling would drift from the one
/// `carve fmt` writes. The body is dropped before the render, so a nested
/// container costs one fence line instead of a second render of its subtree, and
/// the fence runs are then widened to the depth the Markdown writer is at.
pub(crate) fn spell_carrier_markers(node: &BlockNode, depth: usize) -> Option<CarrierMarkers> {
    if let BlockNode::Div(div) = node {
        if div.label.is_none() && div.attrs.as_ref().is_some_and(|attrs| attrs.classes.iter().any(|class| class == "hardbreaks"))
            && div.children.iter().all(|child| !matches!(child, BlockNode::Paragraph(p) if p.children.iter().any(|inline| matches!(inline, crate::ast::InlineNode::SoftBreak(_))))) {
            // The AST records a hard-break block as a div with a structural class.
            let mut attrs = div.attrs.clone();
            if let Some(attrs) = &mut attrs {
                if let Some(index) = attrs.classes.iter().position(|class| class == "hardbreaks") { attrs.classes.remove(index); }
            }
            let prelude = crate::render_carve::render_attrs(&attrs);
            return Some(CarrierMarkers {
                prelude: if prelude.is_empty() { Vec::new() } else { vec![prelude] },
                opener: format!("{} \\", ":".repeat(3 + depth)),
                closer: ":".repeat(3 + depth),
            });
        }
    }
    let bodyless = without_children(node)?;
    let document = Document {
        frontmatter: Default::default(),
        frontmatter_raw: None,
        footnote_defs: Default::default(),
        footnote_def_pos: Default::default(),
        children: vec![bodyless],
        source_len: 0,
        ingest_payload_len: 0,
    };
    let rendered = crate::render_carve::render_carve(&document).ok()?;
    let lines: Vec<&str> = rendered.split('\n').collect();
    let mut open = None;
    let mut close = None;
    for (at, line) in lines.iter().enumerate() {
        if carrier_fence_width(line) == 0 {
            continue;
        }
        if open.is_none() {
            open = Some(at);
        }
        if carrier_is_bare_fence(line) {
            close = Some(at);
        }
    }
    let (open, close) = (open?, close?);
    if close == open {
        return None;
    }
    let widen = |line: &str| {
        let width = carrier_fence_width(line);
        format!("{}{}", ":".repeat(3 + depth), &line[width..])
    };

    Some(CarrierMarkers {
        prelude: lines[..open].iter().map(|l| (*l).to_owned()).collect(),
        opener: widen(lines[open]),
        closer: widen(lines[close]),
    })
}

/// The same container with its body dropped, for the opener-only render.
fn without_children(node: &BlockNode) -> Option<BlockNode> {
    Some(match node {
        BlockNode::Admonition(admonition) => {
            let mut bare = admonition.clone();
            bare.children = Vec::new();
            BlockNode::Admonition(bare)
        }
        BlockNode::Directive(directive) => {
            let mut bare = directive.clone();
            bare.children = Vec::new();
            BlockNode::Directive(bare)
        }
        BlockNode::Div(div) => {
            let mut bare = div.clone();
            bare.children = Vec::new();
            BlockNode::Div(bare)
        }
        BlockNode::FigureGroup(group) => {
            let mut bare = group.clone();
            bare.children = Vec::new();
            // The caption marker belongs below the closer, outside the opener payload.
            bare.caption = None;
            BlockNode::FigureGroup(bare)
        }
        BlockNode::LineBlock(line) => {
            let mut bare = line.clone();
            bare.children.clear();
            BlockNode::LineBlock(bare)
        }
        _ => return None,
    })
}

/// How many bold fallback lines the Markdown target wrote for an opener's own
/// metadata: one for a quoted title, one for a `[label]`. The payload carries
/// them now, so leaving the fallback would be the same text twice.
fn carrier_fallback_lines(payload: &str) -> usize {
    static LABEL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static TITLE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let label = LABEL.get_or_init(|| regex::Regex::new(r"[ \t]?\[.*\]$").unwrap());
    let title = TITLE.get_or_init(|| regex::Regex::new(r#"[ \t]"[^"]*"$"#).unwrap());
    let width = carrier_fence_width(payload);
    if width == 0 {
        return 0;
    }
    let mut rest = &payload[width..];
    let mut lines = 0;
    if let Some(found) = label.find(rest) {
        lines += 1;
        rest = &rest[..found.start()];
    }
    if title.is_match(rest) {
        lines += 1;
    }

    lines
}

/// Classify bare fences in a balanced marker set: every opener closed by a
/// bare fence of its own width, every attribute line against an opener, and
/// nothing left open.
fn carrier_set_closers(payloads: &[String]) -> Option<Vec<bool>> {
    let mut open: Vec<usize> = Vec::new();
    let mut closers = Vec::with_capacity(payloads.len());
    for payload in payloads {
        let width = carrier_fence_width(payload);
        // A bare fence opens a div unless it matches the innermost open fence.
        let closer = carrier_is_bare_fence(payload) && open.last() == Some(&width);
        closers.push(closer);
        if width == 0 {
            continue;
        }
        if closer {
            open.pop();
            continue;
        }
        if open.last().is_some_and(|outer| width <= *outer) {
            return None;
        }
        open.push(width);
    }
    open.is_empty().then_some(closers)
}

fn list_style(payload: &str, start: Option<usize>) -> Option<OrderedListType> {
    let markers: Vec<&str> = payload.split(' ').collect();
    if markers.is_empty()
        || markers.len() > 2
        || markers.iter().any(|marker| {
            let Some(stem) = marker.strip_suffix(['.', ')']) else {
                return true;
            };
            stem.is_empty() || !stem.bytes().all(|ch| ch.is_ascii_alphabetic())
        })
    {
        return None;
    }
    if markers.len() == 1 {
        let letter = markers[0].as_bytes()[0] as char;
        if markers[0].len() == 2 && "ivxlcdmIVXLCDM".contains(letter) {
            if let Some(start) = start {
                let roman = match letter.to_ascii_lowercase() {
                    'i' => 1,
                    'v' => 5,
                    'x' => 10,
                    'l' => 50,
                    'c' => 100,
                    'd' => 500,
                    'm' => 1000,
                    _ => unreachable!(),
                };
                if roman == start {
                    return Some(if letter.is_ascii_lowercase() {
                        OrderedListType::LowerRoman
                    } else {
                        OrderedListType::UpperRoman
                    });
                }
                if letter.to_ascii_lowercase() as usize - 'a' as usize + 1 == start {
                    return Some(if letter.is_ascii_lowercase() {
                        OrderedListType::LowerAlpha
                    } else {
                        OrderedListType::UpperAlpha
                    });
                }
            }
        }
    }
    let sample = markers
        .iter()
        .map(|marker| format!("{marker} item\n"))
        .collect::<String>();
    let parsed = crate::parse::parse(&sample);
    match parsed.children.as_slice() {
        [BlockNode::List(list)] if list.ordered && list.items.len() == markers.len() => {
            list.ol_type
        }
        _ => None,
    }
}

fn host_opens_list(host: &str) -> bool {
    let tail = host.split_whitespace().last().unwrap_or_default();
    if matches!(tail, "-" | "+" | "*") {
        return true;
    }
    let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && digits + 1 == tail.len() && matches!(tail.as_bytes()[digits], b'.' | b')')
}

fn style_in_empty_footnote(lines: &[&str]) -> bool {
    let mut fence: Option<(u8, usize)> = None;
    for (at, line) in lines.iter().enumerate() {
        let body = line.trim_start_matches(' ');
        let indent = line.len() - body.len();
        if indent <= 3 {
            let run = body
                .bytes()
                .take_while(|ch| *ch == b'`' || *ch == b'~')
                .count();
            if run >= 3
                && body.as_bytes()[..run]
                    .iter()
                    .all(|ch| *ch == body.as_bytes()[0])
            {
                let current = (body.as_bytes()[0], run);
                if fence.is_none() {
                    fence = Some(current);
                } else if fence.is_some_and(|(ch, width)| {
                    ch == current.0 && run >= width && body[run..].trim().is_empty()
                }) {
                    fence = None;
                }
                continue;
            }
        }
        if fence.is_some() || indent > 3 || !body.starts_with("[^") {
            continue;
        }
        let Some((label, rest)) = body.split_once("]:") else {
            continue;
        };
        if label.len() <= 2 || !rest.trim().is_empty() {
            continue;
        }
        if lines
            .get(at + 1)
            .and_then(|next| next.strip_prefix("    "))
            .and_then(carrier_payload)
            .is_some_and(|payload| list_style(&payload, None).is_some())
        {
            return true;
        }
    }
    false
}

struct MarkdownBlock {
    range: std::ops::Range<usize>,
    parent: Vec<usize>,
    paragraph: bool,
    ordered_list: bool,
    item: bool,
    captionable: bool,
    host: bool,
    caption_units: usize,
    other_inline: bool,
    math: Option<String>,
}

fn markdown_blocks(markdown: &str) -> Vec<MarkdownBlock> {
    use pulldown_cmark::{Event, Parser, Tag};
    let mut blocks = Vec::new();
    let mut stack: Vec<(bool, Option<usize>, Option<usize>)> = Vec::new();
    let mut hosts = Vec::new();
    let mut image_depth = 0usize;
    let mut image_stack = Vec::new();
    for (event, range) in Parser::new_ext(markdown, markdown_options()).into_offset_iter() {
        // Tight items omit paragraph events, but their inline runs still own captions.
        if let Some((_, index, Some(item))) = stack.last_mut() {
            let inline = matches!(
                &event,
                Event::Text(_)
                    | Event::Code(_)
                    | Event::InlineMath(_)
                    | Event::DisplayMath(_)
                    | Event::InlineHtml(_)
                    | Event::SoftBreak
                    | Event::HardBreak
                    | Event::FootnoteReference(_)
                    | Event::Start(
                        Tag::Emphasis
                            | Tag::Strong
                            | Tag::Strikethrough
                            | Tag::Link { .. }
                            | Tag::Image { .. }
                    )
            );
            if inline && *index == Some(*item) {
                *index = Some(blocks.len());
                blocks.push(MarkdownBlock {
                    range: range.clone(),
                    parent: hosts.clone(),
                    paragraph: true,
                    ordered_list: false,
                    item: false,
                    captionable: false,
                    host: false,
                    caption_units: 0,
                    other_inline: false,
                    math: None,
                });
            } else if !inline {
                *index = Some(*item);
            }
        }
        if let Some(index) = stack
            .iter()
            .rev()
            .find_map(|(_, index, item)| item.filter(|item| *index != Some(*item)).and(*index))
        {
            blocks[index].range.end = range.end;
        }
        match event {
            Event::Start(tag) => {
                let image = matches!(tag, Tag::Image { .. });
                image_stack.push(image);
                image_depth += usize::from(image);
                let host = matches!(
                    tag,
                    Tag::BlockQuote(_) | Tag::Item | Tag::FootnoteDefinition(_)
                );
                let block = matches!(
                    tag,
                    Tag::Paragraph
                        | Tag::Heading { .. }
                        | Tag::CodeBlock(_)
                        | Tag::BlockQuote(_)
                        | Tag::List(_)
                        | Tag::Table(_)
                        | Tag::Item
                        | Tag::FootnoteDefinition(_)
                );
                let index = block.then(|| {
                    let index = blocks.len();
                    blocks.push(MarkdownBlock {
                        range: range.clone(),
                        parent: hosts.clone(),
                        host,
                        caption_units: 0,
                        other_inline: false,
                        math: None,
                        paragraph: matches!(tag, Tag::Paragraph),
                        ordered_list: matches!(tag, Tag::List(Some(_))),
                        item: matches!(tag, Tag::Item),
                        captionable: matches!(
                            tag,
                            Tag::BlockQuote(_) | Tag::CodeBlock(_) | Tag::Table(_)
                        ),
                    });
                    index
                });
                if image_depth == 0
                    && matches!(
                        tag,
                        Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. }
                    )
                {
                    if let Some(index) = stack.iter().rev().find_map(|(_, index, _)| *index) {
                        blocks[index].other_inline = true;
                    }
                }
                if matches!(tag, Tag::Image { .. }) {
                    if let Some(index) = stack.iter().rev().find_map(|(_, index, _)| *index) {
                        blocks[index].captionable = true;
                        blocks[index].caption_units += 1;
                    }
                }
                if host {
                    hosts.push(range.start);
                }
                stack.push((
                    host,
                    index,
                    matches!(tag, Tag::Item).then_some(index).flatten(),
                ));
            }
            Event::End(_) => {
                if image_stack.pop() == Some(true) {
                    image_depth -= 1;
                }
                if let Some((host, _, _)) = stack.pop() {
                    if host {
                        hosts.pop();
                    }
                }
            }
            Event::Rule => blocks.push(MarkdownBlock {
                range,
                parent: hosts.clone(),
                paragraph: false,
                ordered_list: false,
                item: false,
                captionable: false,
                host: false,
                caption_units: 0,
                other_inline: false,
                math: None,
            }),
            Event::DisplayMath(content) => {
                if let Some(index) = stack.iter().rev().find_map(|(_, index, _)| *index) {
                    blocks[index].captionable = true;
                    blocks[index].caption_units += 1;
                    blocks[index].math = Some(content.to_string());
                }
            }
            Event::Text(text) => {
                if !text.trim().is_empty() && image_depth == 0 {
                    if let Some(index) = stack.iter().rev().find_map(|(_, index, _)| *index) {
                        blocks[index].other_inline = true;
                    }
                }
            }
            Event::Code(_)
            | Event::InlineMath(_)
            | Event::InlineHtml(_)
            | Event::SoftBreak
            | Event::HardBreak
                if image_depth == 0 =>
            {
                if let Some(index) = stack.iter().rev().find_map(|(_, index, _)| *index) {
                    blocks[index].other_inline = true;
                }
            }
            _ => {}
        }
    }
    for block in &mut blocks {
        if block.paragraph {
            block.captionable =
                block.captionable && block.caption_units == 1 && !block.other_inline;
        }
    }
    blocks
}

fn markdown_options() -> pulldown_cmark::Options {
    use pulldown_cmark::Options;
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
}

fn marker_parent(marker: &FoundMarker, blocks: &[MarkdownBlock]) -> Vec<usize> {
    blocks
        .iter()
        .filter(|block| block.host && block.range.contains(&marker.offset))
        .max_by_key(|block| block.parent.len())
        .map(|block| {
            let mut parent = block.parent.clone();
            parent.push(block.range.start);
            parent
        })
        .unwrap_or_default()
}

struct ValidatedMarkers {
    dropped: Vec<bool>,
    math: Vec<FoundMarker>,
}

fn math_source(content: &str) -> Option<String> {
    let html = format!(
        "<pre><code>{}</code></pre>",
        content.replace('<', "&lt;").replace('>', "&gt;")
    );
    let imported = crate::html_import::html_to_ast(&html, &Default::default())
        .ok()?
        .value;
    let BlockNode::CodeBlock(code) = imported.children.first()? else {
        return None;
    };
    let mut doc = crate::parse::parse("");
    doc.children
        .push(BlockNode::Paragraph(crate::ast::Paragraph {
            children: vec![crate::ast::InlineNode::Math(crate::ast::Math {
                cell_wraps: None,
                attrs: None,
                display: true,
                content: code.content.clone(),
                label: None,
                number: None,
                pos: None,
            })],
            ..Default::default()
        }));
    crate::render_carve::render_carve(&doc)
        .ok()
        .map(|source| source.trim_end().to_owned())
}

fn validate_markers(
    markdown: &str,
    lines: &[&str],
    markers: &[FoundMarker],
) -> Option<ValidatedMarkers> {
    let blocks = markdown_blocks(markdown);
    let mut dropped = vec![false; lines.len()];
    let mut math = Vec::new();
    let mut fences: Vec<(usize, String)> = Vec::new();
    let mut definitions: std::collections::BTreeSet<(Vec<usize>, usize, usize)> =
        Default::default();
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in lines {
        starts.push(offset);
        offset += line.len() + 1;
    }
    for (index, marker) in markers.iter().enumerate() {
        let payload = &marker.payload;
        let parent = marker_parent(marker, &blocks);
        let next_marker = markers.get(index + 1);
        let next = blocks
            .iter()
            .filter(|block| block.range.start >= marker.end && block.parent == parent)
            .min_by_key(|block| block.range.start);
        let directly_below = next.filter(|block| {
            let line = starts.partition_point(|start| *start <= block.range.start) - 1;
            line == marker.line + 1
                && next_marker.map_or(true, |next| next.offset > block.range.start)
        });
        if payload.starts_with('{') {
            let container_prelude = next_marker.is_some_and(|next| {
                next.line == marker.line + 1
                    && marker_parent(next, &blocks) == parent
                    && (carrier_fence_width(&next.payload) > 0
                        || next.payload.starts_with(":: ")
                        || code_fence_marker_info(&next.payload).is_some()
                        || list_style(&next.payload, None).is_some())
                    && !next.closer
            });
            if !container_prelude && directly_below.is_none() {
                return None;
            }
            let parsed = crate::parse::parse(&format!("{payload}\nx\n"));
            if !matches!(parsed.children.first(), Some(BlockNode::Paragraph(p)) if p.attrs.is_some())
            {
                return None;
            }
        } else if payload.starts_with(":: ") || payload.starts_with("^ ") {
            let paragraph = directly_below.filter(|block| block.paragraph)?;
            let first = starts.partition_point(|start| *start <= paragraph.range.start) - 1;
            let last = starts.partition_point(|start| *start < paragraph.range.end);
            if markers.iter().any(|other| {
                other.offset >= paragraph.range.start && other.offset < paragraph.range.end
            }) {
                return None;
            }
            for drop in &mut dropped[first..last] {
                *drop = true;
            }
            if payload.starts_with(":: ") {
                if !matches!(
                    crate::parse::parse(&format!("{payload}\n: x\n"))
                        .children
                        .first(),
                    Some(BlockNode::DefinitionList(_))
                ) {
                    return None;
                }
                if marker.depth > 1
                    && !definitions.contains(&(parent.clone(), fences.len(), marker.depth - 1))
                {
                    return None;
                }
                let key = (parent, fences.len(), marker.depth);
                definitions.insert(key);
            } else {
                if !matches!(
                    crate::parse::parse(&format!("```\nx\n```\n{payload}\n"))
                        .children
                        .first(),
                    Some(BlockNode::Figure(_))
                ) {
                    return None;
                }
                let prior_marker = index.checked_sub(1).and_then(|i| markers.get(i));
                let group = prior_marker.is_some_and(|prior| {
                    prior.closer
                        && marker_parent(prior, &blocks) == parent
                        && prior.line < marker.line
                        && lines[prior.line + 1..marker.line]
                            .iter()
                            .all(|line| strip_host_prefix(line, &marker.host).is_empty())
                });
                let prior = blocks
                    .iter()
                    .filter(|block| block.range.end <= marker.offset && block.parent == parent)
                    .filter(|block| {
                        !dropped[starts.partition_point(|start| *start <= block.range.start) - 1]
                    })
                    .max_by_key(|block| block.range.end);
                if !group
                    && !prior.is_some_and(|block| {
                        block.captionable
                            && !markers.iter().any(|other| {
                                other.offset >= block.range.end && other.offset < marker.offset
                            })
                            && markdown[block.range.end..marker.offset]
                                .lines()
                                .all(|line| strip_host_prefix(line, &marker.host).trim().is_empty())
                    })
                {
                    return None;
                }
                if let Some(block) = prior.filter(|block| block.math.is_some()) {
                    let line = starts.partition_point(|start| *start <= block.range.start) - 1;
                    let last = starts.partition_point(|start| *start < block.range.end);
                    for drop in &mut dropped[line + 1..last] {
                        *drop = true;
                    }
                    math.push(FoundMarker {
                        line,
                        host: lines[line][..block.range.start - starts[line]].to_owned(),
                        payload: math_source(block.math.as_deref()?)?,
                        depth: 1,
                        offset: block.range.start,
                        end: block.range.end,
                        closer: false,
                    });
                }
            }
        } else if code_fence_marker_info(payload).is_some() {
            if marker.depth != 1
                || !directly_below.is_some_and(|block| {
                    block.captionable
                        && !block.paragraph
                        && matches!(
                            markdown.as_bytes().get(block.range.start),
                            Some(b'`' | b'~')
                        )
                })
            {
                return None;
            }
        } else if list_style(payload, None).is_some() {
            if marker.depth != 1
                || host_opens_list(&marker.host)
                || !directly_below.is_some_and(|block| block.ordered_list)
                || !lines.get(marker.line + 1).is_some_and(|line| {
                    let line = strip_host_prefix(line, &marker.host);
                    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
                    digits > 0 && matches!(line.as_bytes().get(digits), Some(b'.' | b')'))
                })
            {
                return None;
            }
        } else if payload == ":" {
            if !definitions.contains(&(parent.clone(), fences.len(), marker.depth)) {
                return None;
            }
            let empty_definition = next_marker.is_some_and(|next| {
                next.line == marker.line + 1
                    && marker_parent(next, &blocks) == parent
                    && (next.payload.is_empty()
                        || next.payload == ":"
                        || next.payload.starts_with(":: "))
            });
            if directly_below.is_none()
                && !empty_definition
                && !next_marker.is_some_and(|next| {
                    next.line == marker.line + 1
                        && !next.closer
                        && carrier_fence_width(&next.payload) > 0
                })
            {
                return None;
            }
        } else if payload.is_empty() {
            if !definitions.remove(&(parent, fences.len(), marker.depth)) {
                return None;
            }
        } else if marker.closer {
            let (_, opener) = fences.pop()?;
            if next_marker.is_some_and(|next| next.payload.starts_with("^ "))
                && opener.trim_start_matches(':').split_whitespace().next() != Some("figure")
            {
                return None;
            }
        } else {
            fences.push((carrier_fence_width(payload), payload.clone()));
        }
    }
    definitions
        .is_empty()
        .then_some(ValidatedMarkers { dropped, math })
}

/// Every carrier marker lifted out of a Markdown source, leaving a placeholder.
pub(crate) struct CarrierLift {
    pub(crate) source: String,
    /// Each lifted payload and whether it is a bare closer, in marker order.
    slots: Vec<(String, bool, usize, String)>,
    pub(crate) token: String,
    styles: Vec<String>,
    /// A set that does not balance records no structure, so nothing is lifted
    /// and the reader is told.
    pub(crate) damaged: bool,
    pub(crate) dropped: Vec<(usize, String)>,
    marker_newline_token: String,
}

/// Lift every carrier marker line out of the source, leaving a placeholder.
///
/// WHICH LINES ARE MARKERS COMES FROM A PARSE, NOT FROM A LINE SCAN. A flat
/// scan cannot tell a marker-shaped line standing at a list item's content
/// column from verbatim text in a code block inside that item, so it either
/// corrupts the verbatim run or refuses the nesting a document most often has.
/// `pulldown_cmark` has already made that call: a marker that is block content
/// arrives as an HTML block whatever host prefix its line carries, and one
/// inside a fenced or indented code block arrives as code text at any indent
/// (markup-carve/carve#2850).
///
/// A set that does not balance is NEVER reconstructed: the source comes back
/// untouched, the markers import as the raw HTML they are, and the caller
/// reports one `carrier-markers-damaged` loss.
pub(crate) fn lift_carrier_markers(markdown: &str) -> CarrierLift {
    let none = |source: &str, damaged: bool| CarrierLift {
        source: source.to_owned(),
        slots: Vec::new(),
        token: String::new(),
        styles: Vec::new(),
        damaged,
        dropped: Vec::new(),
        marker_newline_token: String::new(),
    };
    if !markdown.contains(CARRIER_STEM) {
        return none(markdown, false);
    }
    let normalized = if markdown.contains('\r') {
        markdown.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        markdown.to_owned()
    };
    let mut marker_newline_token = String::from("CARVEMARKERBREAK");
    while normalized.contains(&marker_newline_token) {
        marker_newline_token.push('X');
    }
    let normalized = join_spanned_markers(&normalized, &marker_newline_token);
    let lines: Vec<&str> = normalized.split('\n').collect();
    if style_in_empty_footnote(&lines) {
        return none(markdown, true);
    }
    let mut payloads = parsed_carrier_markers(&normalized, &lines);
    if payloads.is_empty() {
        return none(markdown, false);
    }
    let only: Vec<String> = payloads.iter().map(|found| found.payload.clone()).collect();
    let Some(closers) = carrier_set_closers(&only) else {
        return none(markdown, true);
    };
    for (marker, closer) in payloads.iter_mut().zip(closers) {
        marker.closer = closer;
    }
    let mut pairs = std::collections::HashMap::new();
    let mut reverse_pairs = std::collections::HashMap::new();
    let mut open = Vec::new();
    for (index, marker) in payloads.iter().enumerate() {
        if carrier_fence_width(&marker.payload) == 0 {
            continue;
        }
        if marker.closer {
            let opener = open.pop().unwrap();
            pairs.insert(opener, index);
            reverse_pairs.insert(index, opener);
        } else {
            open.push(index);
        }
    }
    let mut invalid = std::collections::HashSet::new();
    let mut notices = Vec::new();
    for (index, marker) in payloads.iter().enumerate() {
        if invalid.contains(&index) {
            continue;
        }
        let payload = &marker.payload;
        let width = carrier_fence_width(payload);
        let valid = if marker.depth > 1
            && !payload.starts_with(":: ")
            && payload != ":"
            && !payload.is_empty()
        {
            false
        } else if width > 0 && !marker.closer {
            let closer = &payloads[*pairs.get(&index).unwrap()].payload;
            let tree = crate::parse::parse(&format!("{payload}\n{closer}\n"));
            tree.children.len() == 1 && !matches!(tree.children[0], BlockNode::Paragraph(_))
        } else if payload.starts_with('{') && payload.ends_with('}') {
            let tree = crate::parse::parse(&format!("{payload}\nx\n"));
            matches!(tree.children.first(), Some(BlockNode::Paragraph(paragraph)) if paragraph.attrs.is_some())
        } else if code_fence_marker_info(payload).is_some()
            || (list_style(payload, None).is_some() && !host_opens_list(&marker.host))
        {
            true
        } else {
            width > 0
                || payload.starts_with("^ ")
                || payload.starts_with(":: ")
                || payload == ":"
                || payload.is_empty()
        };
        if valid {
            continue;
        }
        invalid.insert(index);
        if let Some(opener) = reverse_pairs.get(&index) {
            invalid.insert(*opener);
        }
        let pair_closer = pairs
            .get(&index)
            .copied()
            .or_else(|| reverse_pairs.contains_key(&index).then_some(index));
        if let Some(closer) = pair_closer {
            let mut caption_line = payloads[closer].line + 1;
            while caption_line < lines.len()
                && strip_host_prefix(lines[caption_line], &payloads[closer].host).is_empty()
            {
                caption_line += 1;
            }
            if let Some((caption_index, _)) = payloads.iter().enumerate().find(|(_, found)| {
                found.line == caption_line
                    && found.host == payloads[closer].host
                    && found.payload.starts_with("^ ")
            }) {
                invalid.insert(caption_index);
            }
        }
        let message = if let Some(closer) = pairs.get(&index) {
            invalid.insert(*closer);
            "Dropped carrier marker pair with an invalid opener"
        } else {
            "Dropped carrier marker with an invalid payload"
        };
        notices.push((
            marker.line + 1,
            format!(
                "{message}: {}",
                lines[marker.line]
                    .strip_prefix(&marker.host)
                    .unwrap_or(lines[marker.line])
            ),
        ));
    }
    payloads = payloads
        .into_iter()
        .enumerate()
        .filter_map(|(index, marker)| (!invalid.contains(&index)).then_some(marker))
        .collect();
    let dropped = validate_markers(&normalized, &lines, &payloads);
    if dropped.is_none() {
        return none(markdown, true);
    }

    let validated = dropped.unwrap();
    let dropped = validated.dropped;
    payloads.extend(validated.math);
    payloads.sort_by_key(|marker| marker.line);
    let mut token = String::from("CARVECARRIER");
    while normalized.contains(&token) {
        token.push('X');
    }
    let mut slots = Vec::new();
    let mut styles = Vec::new();
    let blocks = if payloads
        .iter()
        .any(|found| list_style(&found.payload, None).is_some())
    {
        markdown_blocks(&normalized)
    } else {
        Vec::new()
    };
    let mut pending_style: Option<(usize, String)> = None;
    let mut out: Vec<String> = Vec::new();
    let mut drop_bold = 0usize;
    let mut skip_blank = false;
    let mut host = String::new();
    let mut next = 0;
    for (at, line) in lines.iter().enumerate() {
        if dropped[at] {
            continue;
        }
        if next < payloads.len() && payloads[next].line == at {
            let found = &payloads[next];
            next += 1;
            if list_style(&found.payload, None).is_some()
                && blocks.iter().any(|block| {
                    block.item && marker_parent(found, &blocks).contains(&block.range.start)
                })
            {
                pending_style = Some((styles.len(), found.host.clone()));
                styles.push(found.payload.clone());
                continue;
            }
            // The placeholder keeps the host prefix the marker line carried, so
            // a marker inside a list item or a block quote stays in that host
            // when the lifted source is parsed.
            let math = if found.payload.starts_with("$$") {
                " math"
            } else {
                ""
            };
            if carrier_fence_width(&found.payload) > 0 {
                // Fence placeholders share the body's paragraph so list hosts
                // retain their content column without extra block separators.
                out.push(format!("{}{token}{}Z", found.host, slots.len()));
                if found.closer
                    && lines.get(at + 1).is_some_and(|next| {
                        let body = strip_host_prefix(next, &found.host);
                        !body.trim().is_empty()
                            && !body.starts_with("<!-- carve:")
                            && !crate::parse::carrier_next_is_list_marker(body)
                    })
                {
                    out.push(found.host.trim_end_matches([' ', '\t']).to_owned());
                }
            } else {
                out.push(format!(
                    "{}<!-- {token}{}Z{math} -->",
                    found.host,
                    slots.len()
                ));
            }
            let closer = found.closer;
            drop_bold = if closer {
                0
            } else {
                carrier_fallback_lines(&found.payload)
            };
            skip_blank = false;
            host = found.host.clone();
            slots.push((
                found.payload.clone(),
                closer,
                found.depth,
                found.host.clone(),
            ));
            continue;
        }
        let body = strip_host_prefix(line, &host);
        if drop_bold > 0 && body.len() > 4 && body.starts_with("**") && body.ends_with("**") {
            drop_bold -= 1;
            skip_blank = true;
            continue;
        }
        if skip_blank && body.is_empty() {
            skip_blank = false;
            continue;
        }
        skip_blank = false;
        if let Some((index, prefix)) = pending_style.take() {
            let body = line.strip_prefix(&prefix).unwrap_or(line);
            let digits = body.bytes().take_while(u8::is_ascii_digit).count();
            let start = digits + 2;
            if digits > 0
                && matches!(body.as_bytes().get(digits), Some(b'.' | b')'))
                && matches!(body.as_bytes().get(digits + 1), Some(b' ' | b'\t'))
            {
                out.push(format!(
                    "{}{}{}STYLE{index}Z {}",
                    prefix,
                    &body[..start],
                    token,
                    &body[start..]
                ));
                continue;
            }
        }
        out.push((*line).to_owned());
    }

    CarrierLift {
        source: out.join("\n"),
        slots,
        token,
        styles,
        damaged: false,
        dropped: notices,
        marker_newline_token,
    }
}

fn join_spanned_markers(source: &str, token: &str) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut out = Vec::with_capacity(lines.len());
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        let body = line.trim_start_matches([' ', '\t', '>']);
        if body.starts_with(CARRIER_STEM) && !body.contains("-->") {
            let mut end = at + 1;
            while end < lines.len()
                && end - at <= 64
                && !lines[end].trim().is_empty()
                && !lines[end].contains("-->")
            {
                end += 1;
            }
            if end < lines.len() && end - at <= 64 && !lines[end].trim().is_empty() {
                let host = &line[..line.len() - body.len()];
                let mut joined = line.to_owned();
                for continuation in &lines[at + 1..=end] {
                    joined.push_str(token);
                    joined.push_str(continuation.strip_prefix(host).unwrap_or(continuation));
                }
                if carrier_payload_at(joined.strip_prefix(host).unwrap_or(&joined)).is_some() {
                    out.push(joined);
                    at = end + 1;
                    continue;
                }
            }
        }
        out.push(line.to_owned());
        at += 1;
    }
    out.join("\n")
}

/// A carrier marker the parse reported as block content.
struct FoundMarker {
    /// Index of the line the marker stands on.
    line: usize,
    /// The host prefix that line carries before the marker, `> ` and list
    /// indentation included.
    host: String,
    payload: String,
    depth: usize,
    offset: usize,
    end: usize,
    closer: bool,
}

/// Every carrier marker a parse of `markdown` reports as an HTML BLOCK.
///
/// Reading only HTML blocks is what makes the verbatim cases free: a marker in
/// a code block arrives as code text instead, so it is never a candidate.
fn parsed_carrier_markers(markdown: &str, lines: &[&str]) -> Vec<FoundMarker> {
    use pulldown_cmark::{Event, Parser};
    let mut line_starts = Vec::with_capacity(lines.len());
    let mut offset = 0;
    for line in lines {
        line_starts.push(offset);
        offset += line.len() + 1;
    }
    // The options the import itself parses with, minus the metadata gate: a
    // frontmatter block holds no HTML block either way.
    let options = markdown_options();
    let mut found = Vec::new();
    let take = |at: usize, text: &str| {
        let line = line_starts.partition_point(|start| *start <= at) - 1;
        let host = &lines[line][..at - line_starts[line]];
        carrier_payload_at(text).map(|(payload, depth)| FoundMarker {
            line,
            host: host.to_owned(),
            payload,
            depth,
            offset: at,
            end: at + text.len(),
            closer: false,
        })
    };
    for (event, range) in Parser::new_ext(markdown, options).into_offset_iter() {
        match event {
            Event::Html(html) => {
                // An HTML block can span several lines; a marker is one line of
                // its own, so each line of the block is tested on its merits.
                let mut at = range.start;
                for text in html.split_inclusive('\n') {
                    found.extend(take(at, text.trim_end_matches('\n')));
                    at += text.len();
                }
            }
            // A MARKER A BLOCK PREFIX PUSHED OFF COLUMN ZERO arrives as an
            // inline span: a task item's `[ ] ` is inline content, so an HTML
            // block cannot begin after it, and the comment is read mid-line.
            // Requiring the span to BE the whole line, host prefix aside, is
            // what keeps a marker-shaped span inside running text out of this.
            Event::InlineHtml(html) => {
                let line = line_starts.partition_point(|start| *start <= range.start) - 1;
                let column = range.start - line_starts[line];
                if lines[line].len() == column + html.len() {
                    found.extend(take(range.start, &html));
                }
            }
            _ => {}
        }
    }
    found.sort_by_key(|marker| marker.line);

    found
}

/// A line with its host prefix removed, for the fallback lines travelling under
/// an opener. The recorded prefix is tried first; a block quote writes its blank
/// lines as a bare `>`, which that prefix does not cover.
fn strip_host_prefix<'a>(line: &'a str, host: &str) -> &'a str {
    if !host.is_empty() {
        if let Some(rest) = line.strip_prefix(host) {
            return rest;
        }
        return line.trim_start_matches([' ', '\t', '>']);
    }

    line
}

/// Write every lifted payload back as the Carve line it is.
pub(crate) fn restore_carrier_markers(carve: &str, lift: &CarrierLift) -> String {
    use pulldown_cmark::{Event, Tag};
    let mut source_with_definitions = carve.to_owned();
    // The import writes a resolved reference inline, so only a definition no
    // link or image used needs its line back.
    let used_ids: Vec<String> = pulldown_cmark::Parser::new_ext(&lift.source, markdown_options())
        .filter_map(|event| match event {
            Event::Start(Tag::Link { id, .. } | Tag::Image { id, .. }) if !id.is_empty() => {
                Some(id.into_string())
            }
            _ => None,
        })
        .collect();
    let parser = pulldown_cmark::Parser::new_ext(&lift.source, markdown_options());
    let definitions = parser.reference_definitions();
    let used: std::collections::HashSet<usize> = used_ids
        .iter()
        .filter_map(|id| definitions.get(id))
        .map(|definition| definition.span.start)
        .collect();
    let mut entries: Vec<_> = definitions.iter().collect();
    entries.sort_by_key(|(_, definition)| definition.span.start);
    for (label, definition) in entries {
        if used.contains(&definition.span.start)
            || !crate::parse::reference_label_has_definition_spelling(label)
            || definition.dest.is_empty()
            || definition
                .title
                .as_ref()
                .is_some_and(|title| title.contains(['\r', '\n']))
        {
            continue;
        }
        let title = definition
            .title
            .as_ref()
            .map(|title| format!(" \"{}\"", title.replace('\\', "\\\\").replace('"', "\\\"")))
            .unwrap_or_default();
        // A document with no block before it writes no blank line above it.
        if source_with_definitions.trim_start_matches('\n').is_empty() {
            source_with_definitions.clear();
        } else {
            source_with_definitions.push('\n');
        }
        source_with_definitions.push_str(&format!(
            "[{label}]: {}{title}\n",
            crate::render_carve::escape_destination(&definition.dest)
        ));
    }
    let carve = source_with_definitions.as_str();
    if lift.slots.is_empty() {
        return if lift.marker_newline_token.is_empty() {
            carve.to_owned()
        } else {
            carve.replace(&lift.marker_newline_token, "\n")
        };
    }
    let items: Vec<RestoredLine> = carve
        .split('\n')
        .map(|line| {
            // The host prefix comes off the CARVE line rather than off the
            // Markdown the placeholder was lifted from: the two hosts nest the
            // same way but spell their prefixes differently, and it is the
            // Carve line the marker has to stand on. The prefix is taken as
            // WHATEVER STANDS BEFORE THE TOKEN rather than as a character
            // class, because a container opening a list item puts the marker
            // behind that item's `-` and leaving the token in the output would
            // be corruption rather than a missed restore.
            let found = line.find(&lift.token);
            let (lead, body) = line.split_at(found.unwrap_or(0));
            let slot = found
                .map(|_| body.trim_end())
                .and_then(|body| body.strip_prefix(&lift.token))
                .and_then(|rest| rest.strip_suffix('Z'))
                .and_then(|index| index.parse::<usize>().ok())
                .and_then(|index| lift.slots.get(index));
            match slot {
                Some((payload, closer, depth, host)) => RestoredLine {
                    lead: if code_fence_marker_info(payload).is_some() && !host.is_empty() {
                        host.clone()
                    } else {
                        format!(
                            "{}{}",
                            lead.strip_suffix("%% ").unwrap_or(lead),
                            "  ".repeat(depth.saturating_sub(1))
                        )
                    },
                    text: payload.clone(),
                    kind: Some(*closer),
                },
                // A line that is not a marker keeps its own spelling, and
                // only its host prefix is needed, for the blank test below.
                None => {
                    let lead = line.len() - line.trim_start_matches([' ', '\t', '>']).len();
                    RestoredLine {
                        lead: line[..lead].to_owned(),
                        text: line[lead..].to_owned(),
                        kind: None,
                    }
                }
            }
        })
        .collect();

    let separated = separate_carrier_lines(&items);
    let continued = restore_container_continuations(&separated);
    let restored = restore_list_styles(&restore_fence_markers(&continued));
    let restored = restore_definition_lines(&restored).join("\n");
    if lift.marker_newline_token.is_empty() {
        restored
    } else {
        restored.replace(&lift.marker_newline_token, "\n")
    }
}

pub(crate) fn restore_nested_list_styles(carve: &str, lift: &CarrierLift) -> Option<String> {
    if lift.styles.is_empty() {
        return Some(carve.to_owned());
    }
    let mut lines: Vec<String> = carve.split('\n').map(str::to_owned).collect();
    for (style_index, payload) in lift.styles.iter().enumerate() {
        let token = format!("{}STYLE{style_index}Z", lift.token);
        let at = lines.iter().position(|line| line.contains(&token))?;
        let token_at = lines[at].find(&token)?;
        let before = &lines[at][..token_at];
        let prefix_len = before
            .bytes()
            .take_while(|ch| matches!(ch, b' ' | b'\t' | b'>'))
            .count();
        let prefix = before[..prefix_len].to_owned();
        let marker = &before[prefix_len..];
        let digits = marker.bytes().take_while(u8::is_ascii_digit).count();
        let delimiter = marker.as_bytes().get(digits).copied()?;
        if digits == 0
            || !matches!(delimiter, b'.' | b')')
            || !marker[digits + 1..].chars().all(char::is_whitespace)
        {
            return None;
        }
        let start = marker[..digits].parse::<usize>().ok()?;
        let style = list_style(payload, Some(start))?;
        let token_end = token_at + token.len();
        let end = token_end + usize::from(lines[at].as_bytes().get(token_end) == Some(&b' '));
        lines[at].replace_range(token_at..end, "");
        let mut counter = start;
        for line in lines.iter_mut().skip(at) {
            let Some(rest) = line.strip_prefix(&prefix) else {
                break;
            };
            if line.contains(&lift.token) {
                if rest.starts_with([' ', '\t']) {
                    continue;
                }
                break;
            }
            let width = rest.bytes().take_while(u8::is_ascii_digit).count();
            if width == 0
                || rest.as_bytes().get(width) != Some(&delimiter)
                || !matches!(rest.as_bytes().get(width + 1), Some(b' ' | b'\t'))
            {
                if rest.is_empty() || rest.starts_with([' ', '\t']) {
                    continue;
                }
                break;
            }
            if rest[..width].parse::<usize>().ok() != Some(counter) {
                break;
            }
            *line = format!(
                "{}{}{}{}",
                prefix,
                crate::render_carve::ordered_marker(counter, Some(style)),
                delimiter as char,
                &rest[width + 1..]
            );
            counter = counter.saturating_add(1);
        }
    }
    let restored = lines.join("\n");
    (!restored.contains(&lift.token)).then_some(restored)
}

fn restore_list_styles(lines: &[RestoredLine]) -> Vec<RestoredLine> {
    let mut written = lines.to_vec();
    let mut remove = vec![false; lines.len()];
    for (at, marker) in lines.iter().enumerate() {
        if marker.kind != Some(false) || list_style(&marker.text, None).is_none() {
            continue;
        }
        let Some(first) = lines.get(at + 1) else {
            continue;
        };
        let line = first.spelled();
        let Some(rest) = line.strip_prefix(&marker.lead) else {
            continue;
        };
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let Some(delimiter) = rest.as_bytes().get(digits).copied() else {
            continue;
        };
        if digits == 0
            || !matches!(delimiter, b'.' | b')')
            || !matches!(rest.as_bytes().get(digits + 1), Some(b' ' | b'\t'))
        {
            continue;
        }
        let Ok(start) = rest[..digits].parse::<usize>() else {
            continue;
        };
        let Some(style) = list_style(&marker.text, Some(start)) else {
            continue;
        };
        let mut counter = start;
        for item in written.iter_mut().skip(at + 1) {
            if item.kind.is_some() {
                break;
            }
            let spelled = item.spelled();
            let Some(rest) = spelled.strip_prefix(&marker.lead) else {
                break;
            };
            let width = rest.bytes().take_while(u8::is_ascii_digit).count();
            if width == 0
                || rest.as_bytes().get(width) != Some(&delimiter)
                || !matches!(rest.as_bytes().get(width + 1), Some(b' ' | b'\t'))
            {
                if rest.is_empty() || rest.starts_with([' ', '\t']) {
                    continue;
                }
                break;
            }
            if rest[..width].parse::<usize>().ok() != Some(counter) {
                break;
            }
            item.lead = marker.lead.clone();
            item.text = format!(
                "{}{}{}",
                crate::render_carve::ordered_marker(counter, Some(style)),
                delimiter as char,
                &rest[width + 1..]
            );
            counter = counter.saturating_add(1);
        }
        remove[at] = true;
    }
    written
        .into_iter()
        .enumerate()
        .filter_map(|(at, line)| (!remove[at]).then_some(line))
        .collect()
}

fn restore_fence_markers(lines: &[RestoredLine]) -> Vec<RestoredLine> {
    let mut out = lines.to_vec();
    let mut removed = vec![false; out.len()];
    for at in 0..out.len() {
        if out[at].kind != Some(false) {
            continue;
        }
        let Some(info) = code_fence_marker_info(&out[at].text) else {
            continue;
        };
        let info = info.to_owned();
        let mut below = at + 1;
        while below < out.len() && (out[below].blank() || out[below].text == "+") {
            below += 1;
        }
        let Some(opener) = out.get_mut(below) else {
            continue;
        };
        let delimiter = opener.text.as_bytes().first().copied().unwrap_or_default();
        let run = opener
            .text
            .bytes()
            .take_while(|byte| *byte == delimiter)
            .count();
        if run < 3 || !matches!(delimiter, b'`' | b'~') {
            continue;
        }
        let language = opener.text[run..].trim();
        let separator = if language.is_empty() { "" } else { " " };
        opener.text = format!("{}{language}{separator}{info}", &opener.text[..run]);
        let host = out[at].lead.clone();
        if !host.is_empty() && host.trim().is_empty() && out[below].lead.is_empty() {
            for line in out.iter_mut().skip(below) {
                line.lead = format!("{host}{}", line.lead);
                if line.text.bytes().all(|byte| byte == delimiter) && line.text.len() >= run {
                    break;
                }
            }
            for index in at + 1..below {
                if out[index].text == "+" {
                    removed[index] = true;
                }
            }
            if at > 0 && out[at - 1].text == "+" {
                out[at - 1].text.clear();
            }
        }
        removed[at] = true;
    }
    out.into_iter()
        .enumerate()
        .filter_map(|(index, line)| (!removed[index]).then_some(line))
        .collect()
}

fn restore_container_continuations(lines: &[RestoredLine]) -> Vec<RestoredLine> {
    use crate::ast::InlineNode;
    use crate::include_walk::{visit_block_children, SubtreeVisitor};

    let mut bare_divs = Vec::new();
    let mut candidates = std::collections::HashSet::new();
    for (index, line) in lines.iter().enumerate() {
        if line.kind == Some(true) {
            bare_divs.pop();
        } else if line.kind == Some(false) && carrier_fence_width(&line.text) > 0 {
            bare_divs.push(carrier_is_bare_fence(&line.text));
        } else if bare_divs.last() == Some(&true) && line.text.starts_with("\\- ") {
            candidates.insert(index + 1);
        }
    }
    if candidates.is_empty() {
        return lines.to_vec();
    }
    struct Continuations(std::collections::HashSet<usize>);
    impl SubtreeVisitor for Continuations {
        fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
            for block in blocks {
                if let BlockNode::Paragraph(paragraph) = block {
                    for pair in paragraph.children.windows(2) {
                        if let [InlineNode::SoftBreak(_), InlineNode::EscapedText(text)] = pair {
                            if text.value == "-" {
                                if let Some(position) = &text.pos {
                                    self.0.insert(position.start_line);
                                }
                            }
                        }
                    }
                }
                visit_block_children(block, self);
            }
        }
    }
    let source = lines
        .iter()
        .map(RestoredLine::spelled)
        .collect::<Vec<_>>()
        .join("\n");
    let mut document = crate::parse::parse_with_options(
        &source,
        &crate::extension::Options::default().with_positions(true),
    );
    let mut continuations = Continuations(Default::default());
    continuations.blocks(&mut document.children);
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let mut restored = line.clone();
            if candidates.contains(&(index + 1)) && continuations.0.contains(&(index + 1)) {
                // Markdown needs the escape to keep a list marker in a paragraph.
                // Inside the restored fence, Carve reads it as a continuation.
                restored.text.remove(0);
            }
            restored
        })
        .collect()
}

fn restore_definition_lines(lines: &[RestoredLine]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut definition_host: Option<String> = None;
    let mut first = false;
    for (index, line) in lines.iter().enumerate() {
        // The Markdown importer spells an item emptied by a term carrier as
        // `+`. It is not authored content when the next carrier closes that
        // definition list, and leaving it here folds it into the term.
        if line.kind.is_none()
            && line.text == "+"
            && index > 0
            && lines[index - 1].kind == Some(false)
            && lines[index - 1].text.starts_with(":: ")
            && host_opens_list(&lines[index - 1].lead)
            && {
                let opener = &lines[index - 1];
                let marker = opener.lead.split_whitespace().last().unwrap_or_default();
                let host = opener
                    .lead
                    .trim_end()
                    .strip_suffix(marker)
                    .unwrap_or_default();
                line.lead == host
                    && lines.get(index + 1).is_some_and(|next| {
                        next.kind == Some(false) && next.lead == host && next.text.is_empty()
                    })
            }
        {
            continue;
        }
        let marker = line.kind.is_some();
        if marker
            && (line.text.starts_with(":: ") || line.text == ":" || line.text.is_empty())
            && first
        {
            if let Some(host) = &definition_host {
                out.push(format!("{host}: {{empty}}"));
            }
            first = false;
        }
        if marker && line.text.starts_with(":: ") {
            definition_host = None;
            if out.last().is_some_and(|line| line.trim().is_empty()) {
                out.pop();
            }
        }
        if marker && line.text == ":" {
            definition_host = Some(line.lead.clone());
            first = true;
            if out.last().is_some_and(|line| line.trim().is_empty()) {
                out.pop();
            }
            continue;
        }
        if marker && line.text.is_empty() {
            definition_host = None;
            if out.last().is_some_and(|line| !line.trim().is_empty()) {
                out.push(line.lead.trim_end_matches([' ', '\t']).to_owned());
            }
            continue;
        }
        if let Some(def_host) = &definition_host {
            if first && line.blank() {
                continue;
            }
            let spelled = line.spelled();
            let content = spelled.strip_prefix(def_host).unwrap_or(&spelled);
            if first {
                out.push(format!("{def_host}: {content}"));
                first = false;
            } else if line.blank() {
                out.push(def_host.trim_end_matches([' ', '\t']).to_owned());
            } else {
                out.push(format!("{def_host}  {content}"));
            }
        } else {
            out.push(line.spelled());
        }
    }
    out
}

/// A line of the written Carve on the way back: its host prefix, the rest, and
/// which kind of marker produced it, if any. `Some(false)` is an opener or the
/// attribute line travelling with it, `Some(true)` a bare closer.
#[derive(Clone)]
struct RestoredLine {
    lead: String,
    text: String,
    kind: Option<bool>,
}

impl RestoredLine {
    /// A line carrying nothing but its host prefix.
    fn blank(&self) -> bool {
        self.kind.is_none() && self.text.is_empty()
    }

    fn spelled(&self) -> String {
        format!("{}{}", self.lead, self.text)
    }
}

/// A host prefix with any list marker in it blanked out, so a closer can stand
/// at its opener's content column without repeating that item's marker.
fn blanked_lead(lead: &str) -> String {
    lead.chars()
        .map(|ch| if ch == '>' || ch == '\t' { ch } else { ' ' })
        .collect()
}

/// Give every restored marker line the blank lines the canonical writer puts
/// around it: a container's opener and closer hug its body, and what follows a
/// closer is a block of its own.
fn separate_carrier_lines(items: &[RestoredLine]) -> Vec<RestoredLine> {
    // A CLOSER STANDS AT ITS OPENER'S COLUMN. Where a container opens a list
    // item and its body begins with a list, the closer's placeholder is a lazy
    // continuation of that inner item's paragraph, so the written Carve puts it
    // at the inner content column; the container would then close in the wrong
    // host. The opener's own prefix is the answer, with any list marker in it
    // blanked out, because a closer cannot repeat an item's marker.
    let mut open: Vec<String> = Vec::new();
    let leads: Vec<String> = items
        .iter()
        .map(|item| match item.kind {
            Some(false) if carrier_fence_width(&item.text) > 0 => {
                open.push(blanked_lead(&item.lead));
                item.lead.clone()
            }
            Some(true) => open.pop().unwrap_or_else(|| item.lead.clone()),
            _ => item.lead.clone(),
        })
        .collect();
    let items: Vec<RestoredLine> = items
        .iter()
        .zip(leads)
        .map(|(item, lead)| RestoredLine {
            lead,
            text: item.text.clone(),
            kind: item.kind,
        })
        .collect();
    let items = &items[..];
    let mut hugged = vec![false; items.len()];
    let mut at = 0;
    while at < items.len() {
        if !items[at].blank() {
            at += 1;
            continue;
        }
        let mut end = at;
        while end < items.len() && items[end].blank() {
            end += 1;
        }
        let mut before = at;
        let above = loop {
            if before == 0 {
                break None;
            }
            before -= 1;
            if !items[before].blank() {
                break Some(&items[before]);
            }
        };
        let below = items.get(end).filter(|item| item.kind.is_some());
        // A blank above a closer and one below an opener are both INSIDE the
        // container, where the canonical writer puts none. The blank has to
        // share the marker's HOST to be inside it: a `>` line next to a marker
        // standing at column 0 belongs to a block quote of its own.
        let inside = below
            .filter(|item| item.kind == Some(true) || item.text.starts_with("^ "))
            .or_else(|| {
                above.filter(|item| item.kind == Some(false) && !item.text.starts_with("^ "))
            })
            .filter(|marker| {
                items[at..end]
                    .iter()
                    .all(|blank| blank.lead.trim_end() == marker.lead.trim_end())
            });
        if inside.is_some() {
            for slot in hugged.iter_mut().take(end).skip(at) {
                *slot = true;
            }
        }
        at = end;
    }

    let mut out: Vec<RestoredLine> = Vec::new();
    let mut last: Option<&RestoredLine> = None;
    for (index, item) in items.iter().enumerate() {
        if hugged[index] {
            continue;
        }
        // A closer and what follows it are two blocks, so they take a blank
        // line between them - but only where they are SIBLINGS. A list item
        // opening after a closer is a block of the host above, and a blank
        // there would turn a tight list loose.
        if last.is_some_and(|prior| prior.kind == Some(true) && item.lead.len() >= prior.lead.len())
            && item.kind != Some(true)
            && !item.text.starts_with("^ ")
            && !item.text.is_empty()
        {
            out.push(RestoredLine {
                lead: item.lead.trim_end().to_owned(),
                text: String::new(),
                kind: None,
            });
        }
        out.push(item.clone());
        last = Some(item);
    }

    out
}

#[cfg(test)]
mod detached_term_tests {
    use super::{restore_definition_lines, RestoredLine};

    #[test]
    fn repeated_empty_list_term_carriers_drop_only_the_local_placeholder() {
        let mut lines = Vec::new();
        for index in 0..4096 {
            lines.push(RestoredLine {
                lead: "- ".to_owned(),
                text: format!(":: term{index}"),
                kind: Some(false),
            });
            lines.push(RestoredLine {
                lead: String::new(),
                text: "+".to_owned(),
                kind: None,
            });
            lines.push(RestoredLine {
                lead: String::new(),
                text: String::new(),
                kind: Some(false),
            });
            lines.push(RestoredLine {
                lead: String::new(),
                text: format!("after{index}"),
                kind: None,
            });
        }
        let restored = restore_definition_lines(&lines);
        assert_eq!(
            restored
                .iter()
                .filter(|line| line.starts_with("- :: term"))
                .count(),
            4096
        );
        assert_eq!(
            restored
                .iter()
                .filter(|line| line.starts_with("after"))
                .count(),
            4096
        );
        assert!(!restored.iter().any(|line| line == "+"));
    }
}
