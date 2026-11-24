//! Markdown, GFM and Djot import through the HTML hub (ruling of 17
//! November, carve-js#5074): a library renders HTML and the 0.2 HTML importer
//! reads it. The steps here only add what that HTML cannot carry: raw HTML,
//! the tight-list carrier, frontmatter, math, GitHub alerts and a Djot start
//! of 0.

use super::Error;
use serde_json::{Value, json};

/// The input formats of `importDocument` (C2-M-001, carve-js#5074).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    Html,
    /// `CommonMark`.
    Markdown,
    /// GitHub Flavored Markdown.
    Gfm,
    Djot,
}

impl ImportFormat {
    /// Parses the `from` value of the JSON boundary.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "html" => Some(Self::Html),
            "markdown" => Some(Self::Markdown),
            "gfm" => Some(Self::Gfm),
            "djot" => Some(Self::Djot),
            _ => None,
        }
    }
}

/// What the library HTML cannot hold: the frontmatter and the raw blocks
/// that placeholder comments stand for.
#[derive(Default)]
pub(crate) struct Sink {
    frontmatter: Option<(&'static str, String)>,
    raw: Vec<(String, String)>,
    /// The placeholder prefix, made longer until the input does not hold it, so a
    /// comment of the input is never taken for a placeholder.
    marker: String,
}

const PLACEHOLDER: &str = "carve-hub-raw-";

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn placeholder(sink: &mut Sink, format: &str, value: &str) -> String {
    sink.raw.push((format.to_string(), value.to_string()));
    format!("<!--{}{}-->", sink.marker, sink.raw.len() - 1)
}

/// The library HTML of `input`.
///
/// # Errors
///
/// [`Error::ImportInput`] when the library fails.
pub(crate) fn to_html(input: &str, format: ImportFormat, sink: &mut Sink) -> Result<String, Error> {
    let taken = input
        .match_indices(PLACEHOLDER)
        .map(|(at, _)| {
            input[at + PLACEHOLDER.len()..]
                .bytes()
                .take_while(|&b| b == b'x')
                .count()
                + 1
        })
        .max()
        .unwrap_or(0);
    sink.marker = format!("{PLACEHOLDER}{}", "x".repeat(taken));
    match format {
        ImportFormat::Html => Ok(input.to_string()),
        ImportFormat::Markdown | ImportFormat::Gfm => {
            markdown_html(input, format == ImportFormat::Gfm, sink)
        }
        ImportFormat::Djot => Ok(djot_html(input, sink)),
    }
}

pub(super) fn markdown_options(input: &str, gfm: bool) -> comrak::Options<'static> {
    let mut o = comrak::Options::default();
    o.extension.math_dollars = true;
    // comrak skips a leading U+FEFF before its frontmatter scan (carve#6447).
    let start = input.strip_prefix('\u{feff}').unwrap_or(input);
    let toml = start.starts_with("+++\n") || start.starts_with("+++\r\n");
    o.extension.front_matter_delimiter = Some(if toml { "+++" } else { "---" }.into());
    o.render.r#unsafe = true;
    if gfm {
        o.extension.table = true;
        o.extension.strikethrough = true;
        o.extension.tasklist = true;
        o.extension.footnotes = true;
        o.extension.autolink = true;
    }
    o
}

/// Markdown (`CommonMark`) or GFM to HTML with comrak, the cmark-gfm port.
fn markdown_html(input: &str, gfm: bool, sink: &mut Sink) -> Result<String, Error> {
    use comrak::nodes::{NodeHtmlBlock, NodeValue};
    let (input, options) = match split_frontmatter(input) {
        Some((format, value, rest)) => {
            sink.frontmatter = Some((format, value));
            let mut options = markdown_options(rest, gfm);
            options.extension.front_matter_delimiter = None;
            (rest, options)
        }
        None => (input, markdown_options(input, gfm)),
    };
    let input = lf_endings(input);
    let flow = math_flow(&input, &options);
    let (input, math_fence) = flow
        .as_ref()
        .map_or((input.as_ref(), usize::MAX), |(text, floor)| {
            (text.as_str(), *floor)
        });
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, input, &options);
    let mut tight = Vec::new();
    let mut loose = std::collections::HashSet::new();
    let nodes: Vec<_> = root.descendants().collect();
    for node in nodes {
        let value = node.data.borrow().value.clone();
        let html = |text: String| arena.alloc(NodeValue::HtmlInline(text).into());
        match value {
            NodeValue::HtmlBlock(block) => {
                let literal = placeholder(sink, "html", &block.literal) + "\n";
                node.data.borrow_mut().value =
                    NodeValue::HtmlBlock(NodeHtmlBlock { literal, ..block });
            }
            NodeValue::HtmlInline(text) => {
                let span = format!(r#"<span data-carve-raw="html">{}</span>"#, escape(&text));
                node.insert_before(html(span));
                node.detach();
            }
            // micromark reads `$$` in text as inline math; display math is the flow block only.
            NodeValue::Math(math) => {
                let span = format!(
                    r#"<span class="math inline">\({}\)</span>"#,
                    escape(inline_math(&math.literal))
                );
                node.insert_before(html(span));
                node.detach();
            }
            NodeValue::CodeBlock(code)
                if code.fence_char == b'`' && code.fence_length >= math_fence =>
            {
                display_math(node, &code.literal, &mut loose);
            }
            NodeValue::List(list) => tight.push((std::ptr::from_ref(node), list.tight)),
            NodeValue::BlockQuote if gfm => alert(&arena, node),
            _ => {}
        }
    }
    let mut out = String::new();
    comrak::format_html(root, &options, &mut out).map_err(|e| Error::ImportInput {
        message: e.to_string(),
    })?;
    let tight: Vec<bool> = tight
        .into_iter()
        .map(|(list, tight)| tight && !loose.contains(&list))
        .collect();
    Ok(mark_tight(&out, &tight))
}

/// micromark's math flow, which comrak lacks (carve-rs#4639): each `$$` fence
/// line becomes a backtick fence longer than any backtick run of the input, so
/// comrak pairs them as micromark pairs math flow and an unclosed one runs to
/// the end of its container. The text and the shortest such fence; `None` when
/// no line is a math fence.
fn math_flow(input: &str, options: &comrak::Options<'_>) -> Option<(String, usize)> {
    if !input.contains("$$") {
        return None;
    }
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut at = 0;
    for line in input.split_inclusive('\n') {
        lines.push((at, line.strip_suffix('\n').unwrap_or(line)));
        at += line.len();
    }
    let all: Vec<MathFence> = lines.iter().map(|(_, line)| math_fence(line)).collect();
    if all.iter().all(Option::is_none) {
        return None;
    }
    // Only a backtick run that starts a line can pair with a fence.
    let longest = lines
        .iter()
        .map(|(_, line)| {
            let rest = line.get(content_start(line)..).unwrap_or_default();
            rest.bytes().take_while(|&b| b == b'`').count()
        })
        .max()
        .unwrap_or(0);
    let floor = longest + 3;
    let count = all.iter().flatten().count();
    // The rewrite grows the text by a fence per line; past this bound comrak's own reading stays.
    if count.saturating_mul(floor) > input.len().saturating_mul(2) + 65_536 {
        return None;
    }
    // A `$$` line in code or raw HTML is content; left in, its fence could close that block.
    let opaque = read_blocks(input, options, usize::MAX).opaque;
    let mut active: Vec<MathFence> = all
        .iter()
        .enumerate()
        .map(|(index, fence)| fence.filter(|_| !opaque.contains(&(index + 1))))
        .collect();
    // The pairing is read again until it holds, a bounded number of times.
    let mut rejected = vec![false; all.len()];
    for _ in 0..6 {
        let text = rewrite_fences(input, &lines, &active, longest);
        let next = pair(
            &all,
            &active,
            &mut rejected,
            &read_blocks(&text, options, floor),
        );
        if next == active {
            return active.iter().any(Option::is_some).then_some((text, floor));
        }
        active = next;
    }
    None
}

/// The 1-based lines of the math blocks (opener, last line, closed) and of the
/// other code and raw HTML blocks of `text`; a math block is a backtick fence of
/// at least `floor`.
struct Blocks {
    math: Vec<(usize, usize, bool)>,
    opaque: std::collections::HashSet<usize>,
}

fn read_blocks(text: &str, options: &comrak::Options<'_>, floor: usize) -> Blocks {
    use comrak::nodes::NodeValue;
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, text, options);
    let mut blocks = Blocks {
        math: Vec::new(),
        opaque: std::collections::HashSet::new(),
    };
    for node in root.descendants() {
        let data = node.data.borrow();
        let (start, end) = (data.sourcepos.start.line, data.sourcepos.end.line);
        match &data.value {
            NodeValue::CodeBlock(code) if code.fence_char == b'`' && code.fence_length >= floor => {
                blocks.math.push((start, end, code.closed));
            }
            NodeValue::CodeBlock(_) | NodeValue::HtmlBlock(_) => blocks.opaque.extend(start..=end),
            _ => {}
        }
    }
    blocks
}

/// The fence lines of the next reading: the openers and closers of the math blocks,
/// and every other fence line outside every block. A fence line that opened or closed
/// nothing, as comrak read it, is rejected for good.
fn pair(
    all: &[MathFence],
    active: &[MathFence],
    rejected: &mut [bool],
    blocks: &Blocks,
) -> Vec<MathFence> {
    let read: std::collections::HashSet<usize> = blocks
        .math
        .iter()
        .flat_map(|&(start, end, closed)| [Some(start), closed.then_some(end)])
        .flatten()
        .collect();
    for (index, (fence, flag)) in active.iter().zip(rejected.iter_mut()).enumerate() {
        if fence.is_some() && !read.contains(&(index + 1)) {
            *flag = true;
        }
    }
    let (mut next, inside) = paired(all, rejected, blocks);
    for (index, fence) in all.iter().enumerate() {
        let free = !inside.get(index).copied().unwrap_or(true)
            && !blocks.opaque.contains(&(index + 1))
            && !rejected.get(index).copied().unwrap_or(true);
        if free && let Some(slot) = next.get_mut(index) {
            *slot = *fence;
        }
    }
    next
}

/// Each math block keeps its opener and takes the first line of its body that can
/// close it (no meta, a run at least as long); the second value marks the lines of
/// the math blocks.
fn paired(all: &[MathFence], rejected: &[bool], blocks: &Blocks) -> (Vec<MathFence>, Vec<bool>) {
    let mut next = vec![None; all.len()];
    let mut inside = vec![false; all.len()];
    for &(start, end, _) in &blocks.math {
        let Some(&Some(opener)) = all.get(start - 1) else {
            continue;
        };
        if let Some(slot) = next.get_mut(start - 1) {
            *slot = Some(opener);
        }
        for slot in inside.iter_mut().take(end).skip(start - 1) {
            *slot = true;
        }
        let closer = (start + 1..=end)
            .filter(|&line| !rejected.get(line - 1).copied().unwrap_or(true))
            .find_map(|line| match all.get(line - 1) {
                Some(&Some(fence @ (_, run, false))) if run >= opener.1 => Some((line, fence)),
                _ => None,
            });
        if let Some((line, fence)) = closer
            && let Some(slot) = next.get_mut(line - 1)
        {
            *slot = Some(fence);
        }
    }
    (next, inside)
}

type MathFence = Option<(usize, usize, bool)>;

/// `input` with each math fence line written as a backtick fence: `longest` plus one
/// backticks more than its `$` run, and a meta kept as a word so it cannot close.
fn rewrite_fences(
    input: &str,
    lines: &[(usize, &str)],
    fences: &[MathFence],
    longest: usize,
) -> String {
    let mut out = String::with_capacity(input.len() + fences.len() * (longest + 3));
    let mut done = 0;
    for ((start, line), fence) in lines.iter().zip(fences) {
        if let Some((at, run, meta)) = fence {
            out.push_str(input.get(done..start + at).unwrap_or_default());
            out.push_str(&"`".repeat(longest + 1 + run));
            if *meta {
                out.push_str(" m");
            }
            done = start + line.len();
        }
    }
    out.push_str(input.get(done..).unwrap_or_default());
    out
}

/// The content of `$$` math in text as micromark reads it, by the padding rule of a
/// code span: one space or line ending is stripped from each side when both sides
/// have one and the content is not all padding.
fn inline_math(literal: &str) -> &str {
    let pad = |c: char| matches!(c, ' ' | '\n');
    let mut chars = literal.chars();
    match (chars.next(), chars.next_back()) {
        (Some(first), Some(last)) if pad(first) && pad(last) && !literal.chars().all(pad) => {
            &literal[1..literal.len() - 1]
        }
        _ => literal,
    }
}

/// `CommonMark` reads CR and CRLF as line endings, so no node keeps a CR (carve-rs#4640).
fn lf_endings(input: &str) -> std::borrow::Cow<'_, str> {
    if input.contains('\r') {
        std::borrow::Cow::Owned(input.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        std::borrow::Cow::Borrowed(input)
    }
}

/// A math fence code block as display math: a paragraph of its own also in a tight
/// list, which renders no `<p>`, so its list is loose, as the HTML of an item with a
/// paragraph reads (C2-M-033).
fn display_math<'a>(
    node: &'a comrak::nodes::AstNode<'a>,
    literal: &str,
    loose: &mut std::collections::HashSet<*const comrak::nodes::AstNode<'a>>,
) {
    use comrak::nodes::{NodeHtmlBlock, NodeValue};
    let literal = literal.strip_suffix('\n').unwrap_or(literal);
    let span = format!(
        r#"<span class="math display">\[{}\]</span>"#,
        escape(literal)
    );
    node.data.borrow_mut().value = NodeValue::HtmlBlock(NodeHtmlBlock {
        literal: format!("<p>{span}</p>\n"),
        ..NodeHtmlBlock::default()
    });
    if let Some(list) = node
        .parent()
        .filter(|item| matches!(item.data.borrow().value, NodeValue::Item(_)))
        .and_then(comrak::arena_tree::Node::parent)
    {
        loose.insert(std::ptr::from_ref(list));
    }
}

/// The offset and length of the `$` run of a line that can be a math fence
/// behind its container markers, and whether a meta follows it.
fn math_fence(line: &str) -> MathFence {
    let at = content_start(line);
    let rest = line.get(at..)?;
    let run = rest.bytes().take_while(|&b| b == b'$').count();
    let meta = rest.get(run..)?;
    (run >= 2 && !meta.contains('$')).then(|| (at, run, !meta.trim_matches([' ', '\t']).is_empty()))
}

/// Where a line's content starts behind its quote marks, list markers and blanks.
fn content_start(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut at = 0;
    loop {
        let rest = bytes.get(at..).unwrap_or_default();
        let blanks = rest
            .iter()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        at += blanks;
        let rest = rest.get(blanks..).unwrap_or_default();
        if rest.first() == Some(&b'>') {
            at += 1;
            continue;
        }
        let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
        let marker = match rest.first() {
            Some(b'-' | b'+' | b'*') => 1,
            _ if (1..=9).contains(&digits) && matches!(rest.get(digits), Some(b'.' | b')')) => {
                digits + 1
            }
            _ => return at,
        };
        if !matches!(rest.get(marker), Some(b' ' | b'\t')) {
            return at;
        }
        at += marker;
    }
}

/// A closed frontmatter at the start, after at most one U+FEFF: the format,
/// the text between the fences and the input after the closer. The hub reads
/// it instead of comrak, which misses a closer on the opener's next line,
/// blanks after the opener and CR line endings (carve-rs#4633); the rule is
/// micromark-extension-frontmatter's, as carve-js reads it.
fn split_frontmatter(input: &str) -> Option<(&'static str, String, &str)> {
    fn line(text: &str) -> (&str, &str) {
        let end = text.find(['\r', '\n']).unwrap_or(text.len());
        let (line, rest) = text.split_at(end);
        let rest = rest
            .strip_prefix("\r\n")
            .or_else(|| rest.strip_prefix(['\r', '\n']))
            .unwrap_or(rest);
        (line, rest)
    }
    let is_fence = |l: &str, d: &str| {
        l.strip_prefix(d)
            .is_some_and(|r| r.trim_matches([' ', '\t']).is_empty())
    };
    let start = input.strip_prefix('\u{feff}').unwrap_or(input);
    let (opener, mut rest) = line(start);
    let (delimiter, format) = [("---", "yaml"), ("+++", "toml")]
        .into_iter()
        .find(|(d, _)| is_fence(opener, d))?;
    if opener.len() == start.len() {
        return None;
    }
    let mut value = String::new();
    while !rest.is_empty() {
        let (l, after) = line(rest);
        if is_fence(l, delimiter) {
            return Some((format, value, after));
        }
        value.push_str(l);
        value.push('\n');
        rest = after;
    }
    None
}

/// Every list opening tag of the library HTML is a list of the tree in
/// document order: raw HTML went into placeholders and text is escaped. Each
/// tight one gets `data-carve-tight` (C2-M-033).
fn mark_tight(html: &str, tight: &[bool]) -> String {
    let mut out = String::with_capacity(html.len() + tight.len() * 20);
    let mut rest = html;
    let mut flags = tight.iter();
    while let Some(at) = rest.find("<ul").into_iter().chain(rest.find("<ol")).min() {
        let (before, tag) = rest.split_at(at);
        out.push_str(before);
        out.push_str(tag.get(..3).unwrap_or(""));
        if flags.next().copied().unwrap_or(false) {
            out.push_str(r#" data-carve-tight="""#);
        }
        rest = tag.get(3..).unwrap_or("");
    }
    out.push_str(rest);
    out
}

/// A GitHub alert, a quote whose first line is `[!KIND]` alone, is the
/// admonition the 0.1 importer made of it (C2-A-066).
fn alert<'a>(arena: &'a comrak::Arena<'a>, quote: comrak::nodes::Node<'a>) {
    use comrak::nodes::{NodeHtmlBlock, NodeValue};
    let Some(first) = quote.first_child() else {
        return;
    };
    if !matches!(first.data.borrow().value, NodeValue::Paragraph) {
        return;
    }
    let Some(lead) = first.first_child() else {
        return;
    };
    let kind = match &lead.data.borrow().value {
        NodeValue::Text(text) => match text.as_ref() {
            "[!NOTE]" => "note",
            "[!TIP]" => "tip",
            "[!IMPORTANT]" => "info",
            "[!WARNING]" => "warning",
            "[!CAUTION]" => "danger",
            _ => return,
        },
        _ => return,
    };
    let after = lead.next_sibling();
    match after.as_ref().map(|n| n.data.borrow().value.clone()) {
        None => first.detach(),
        Some(NodeValue::SoftBreak | NodeValue::LineBreak) => {
            lead.detach();
            if let Some(n) = after {
                n.detach();
            }
        }
        Some(_) => return,
    }
    let block = |literal: String| {
        arena.alloc(
            NodeValue::HtmlBlock(NodeHtmlBlock {
                literal,
                ..NodeHtmlBlock::default()
            })
            .into(),
        )
    };
    quote.insert_before(block(format!("<aside class=\"admonition {kind}\">\n")));
    let children: Vec<_> = quote.children().collect();
    for child in children {
        quote.insert_before(child);
    }
    quote.insert_before(block("</aside>\n".into()));
    quote.detach();
}

/// Djot to HTML with jotdown; raw content of every format is carried as in
/// Markdown, a start of 0 is kept (C2-B-041), and generated section ids are
/// left out.
fn djot_html(input: &str, sink: &mut Sink) -> String {
    use crate::jotdown::{Attributes, Container, Event, ListKind};
    let mut events = Vec::new();
    let mut section_attrs: Option<Attributes<'_>> = None;
    let mut raw: Option<(String, String)> = None;
    for event in crate::jotdown::Parser::new(input) {
        if let Some((format, text)) = raw.as_mut() {
            match event {
                Event::Str(s) => text.push_str(&s),
                Event::End(Container::RawBlock { .. }) => {
                    let html = placeholder(sink, format, text);
                    events.push(Event::Start(
                        Container::RawBlock {
                            format: "html".into(),
                        },
                        Attributes::new(),
                    ));
                    events.push(Event::Str(format!("{html}\n").into()));
                    events.push(Event::End(Container::RawBlock {
                        format: "html".into(),
                    }));
                    raw = None;
                }
                Event::End(Container::RawInline { .. }) => {
                    let span = format!(
                        r#"<span data-carve-raw="{}">{}</span>"#,
                        escape(format),
                        escape(text)
                    );
                    events.push(Event::Start(
                        Container::RawInline {
                            format: "html".into(),
                        },
                        Attributes::new(),
                    ));
                    events.push(Event::Str(span.into()));
                    events.push(Event::End(Container::RawInline {
                        format: "html".into(),
                    }));
                    raw = None;
                }
                _ => {}
            }
            continue;
        }
        match event {
            Event::Start(Container::RawBlock { format } | Container::RawInline { format }, _) => {
                raw = Some((format.to_string(), String::new()));
            }
            Event::Start(Container::Section { .. }, attrs) => section_attrs = Some(attrs),
            Event::End(Container::Section { .. }) => {}
            Event::Start(c @ Container::Heading { .. }, mut attrs) => {
                if let Some(more) = section_attrs.take() {
                    attrs.extend(more);
                }
                events.push(Event::Start(c, attrs));
            }
            Event::Start(c @ Container::List { kind, tight }, mut attrs) => {
                if tight {
                    attrs.extend(
                        Attributes::try_from(r#"{data-carve-tight=""}"#).unwrap_or_default(),
                    );
                }
                if matches!(kind, ListKind::Ordered { start: 0, .. }) {
                    attrs.extend(Attributes::try_from("{start=0}").unwrap_or_default());
                }
                events.push(Event::Start(c, attrs));
            }
            other => events.push(other),
        }
    }
    crate::jotdown::html::render_to_string(events.into_iter())
}

/// The tree of the HTML importer with the frontmatter in front and each raw
/// placeholder swapped back for the block it stands for.
pub(crate) fn finish(mut tree: Value, sink: &Sink) -> Value {
    if let Some(children) = tree.get_mut("children").and_then(Value::as_array_mut) {
        if !sink.raw.is_empty() {
            restore(children, sink);
        }
        fill_empty_items(children);
        if let Some((format, value)) = &sink.frontmatter {
            children.insert(
                0,
                json!({"type": "frontmatter", "format": format, "value": value}),
            );
        }
    }
    tree
}

/// An empty source item is a marker plus a comment, as a parse of `- %%`
/// gives (C2-A-186); a childless `list_item` has no spelling (C2-W-057).
fn fill_empty_items(nodes: &mut [Value]) {
    for node in nodes {
        let is_item = node.get("type").and_then(Value::as_str) == Some("list_item");
        let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) else {
            continue;
        };
        if is_item && children.is_empty() {
            children.push(json!({"type": "comment", "form": "line", "value": ""}));
        } else {
            fill_empty_items(children);
        }
    }
}

fn restore(nodes: &mut [Value], sink: &Sink) {
    for node in nodes {
        // A block after text in a tight list item reaches the tree in inline position.
        let block = node.get("type").and_then(Value::as_str) == Some("raw_block");
        let inline = node.get("type").and_then(Value::as_str) == Some("raw_inline");
        let kept = (block || inline)
            .then(|| node.get("value").and_then(Value::as_str))
            .flatten()
            .and_then(|v| v.strip_prefix("<!--")?.strip_prefix(sink.marker.as_str()))
            .and_then(|v| {
                v.strip_suffix('\n')
                    .unwrap_or(v)
                    .strip_suffix("-->")?
                    .parse::<usize>()
                    .ok()
            })
            .and_then(|i| sink.raw.get(i));
        if let (Some((format, value)), Some(map)) = (kept, node.as_object_mut()) {
            let value = if inline {
                value.strip_suffix('\n').unwrap_or(value).to_string()
            } else if value.ends_with('\n') {
                value.clone()
            } else {
                format!("{value}\n")
            };
            map.insert("format".into(), format.as_str().into());
            map.insert("value".into(), value.into());
            continue;
        }
        if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
            restore(children, sink);
        }
    }
}
