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
    format!("<!--{PLACEHOLDER}{}-->", sink.raw.len() - 1)
}

/// The library HTML of `input`.
///
/// # Errors
///
/// [`Error::ImportInput`] when the library fails.
pub(crate) fn to_html(input: &str, format: ImportFormat, sink: &mut Sink) -> Result<String, Error> {
    match format {
        ImportFormat::Html => Ok(input.to_string()),
        ImportFormat::Markdown | ImportFormat::Gfm => {
            markdown_html(input, format == ImportFormat::Gfm, sink)
        }
        ImportFormat::Djot => Ok(djot_html(input, sink)),
    }
}

fn markdown_options(input: &str, gfm: bool) -> comrak::Options<'static> {
    let mut o = comrak::Options::default();
    o.extension.math_dollars = true;
    let toml = input.starts_with("+++\n") || input.starts_with("+++\r\n");
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
    let options = markdown_options(input, gfm);
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, input, &options);
    let mut tight = Vec::new();
    let nodes: Vec<_> = root.descendants().collect();
    for node in nodes {
        let value = node.data.borrow().value.clone();
        let html = |text: String| arena.alloc(NodeValue::HtmlInline(text).into());
        match value {
            NodeValue::FrontMatter(text) => {
                sink.frontmatter = Some(frontmatter(&text));
                node.detach();
            }
            NodeValue::HtmlBlock(block) => {
                let literal = placeholder(sink, "html", &block.literal) + "\n";
                node.data.borrow_mut().value = NodeValue::HtmlBlock(NodeHtmlBlock::new(literal));
            }
            NodeValue::HtmlInline(text) => {
                let span = format!(r#"<span data-carve-raw="html">{}</span>"#, escape(&text));
                node.insert_before(html(span));
                node.detach();
            }
            NodeValue::Math(math) => {
                let (class, open, close) = if math.display_math {
                    ("display", "\\[", "\\]")
                } else {
                    ("inline", "\\(", "\\)")
                };
                // A `$$` block holds the line endings after its opener and
                // before its closer; they are no part of the formula.
                let literal = if math.display_math {
                    let l = math.literal.as_str();
                    let l = l
                        .strip_prefix("\r\n")
                        .or_else(|| l.strip_prefix('\n'))
                        .unwrap_or(l);
                    l.strip_suffix("\r\n")
                        .or_else(|| l.strip_suffix('\n'))
                        .unwrap_or(l)
                } else {
                    math.literal.as_str()
                };
                let span = format!(
                    r#"<span class="math {class}">{open}{}{close}</span>"#,
                    escape(literal)
                );
                node.insert_before(html(span));
                node.detach();
            }
            NodeValue::List(list) => tight.push(list.tight),
            NodeValue::BlockQuote if gfm => alert(&arena, node),
            _ => {}
        }
    }
    let mut out = String::new();
    comrak::format_html(root, &options, &mut out).map_err(|e| Error::ImportInput {
        message: e.to_string(),
    })?;
    Ok(mark_tight(&out, &tight))
}

/// `---` or `+++` lines around the frontmatter text.
fn frontmatter(text: &str) -> (&'static str, String) {
    let format = if text.starts_with("+++") {
        "toml"
    } else {
        "yaml"
    };
    let lines: Vec<&str> = text.lines().collect();
    let close = lines
        .iter()
        .rposition(|l| matches!(l.trim_end(), "---" | "+++"))
        .unwrap_or(lines.len());
    let body = lines.get(1..close).unwrap_or(&[]);
    let value = if body.is_empty() {
        String::new()
    } else {
        format!("{}\n", body.join("\n"))
    };
    (format, value)
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
    let block =
        |literal: String| arena.alloc(NodeValue::HtmlBlock(NodeHtmlBlock::new(literal)).into());
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
    use jotdown::{Attributes, Container, Event, ListKind};
    let mut events = Vec::new();
    let mut section_attrs: Option<Attributes<'_>> = None;
    let mut raw: Option<(String, String)> = None;
    for event in jotdown::Parser::new(input) {
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
    jotdown::html::render_to_string(events.into_iter())
}

/// The tree of the HTML importer with the frontmatter in front and each raw
/// placeholder swapped back for the block it stands for.
pub(crate) fn finish(mut tree: Value, sink: &Sink) -> Value {
    if let Some(children) = tree.get_mut("children").and_then(Value::as_array_mut) {
        if !sink.raw.is_empty() {
            restore(children, &sink.raw);
        }
        if let Some((format, value)) = &sink.frontmatter {
            children.insert(
                0,
                json!({"type": "frontmatter", "format": format, "value": value}),
            );
        }
    }
    tree
}

fn restore(nodes: &mut [Value], raw: &[(String, String)]) {
    for node in nodes {
        let kept = (node.get("type").and_then(Value::as_str) == Some("raw_block"))
            .then(|| node.get("value").and_then(Value::as_str))
            .flatten()
            .and_then(|v| v.strip_prefix("<!--")?.strip_prefix(PLACEHOLDER))
            .and_then(|v| v.strip_suffix("-->\n")?.parse::<usize>().ok())
            .and_then(|i| raw.get(i));
        if let (Some((format, value)), Some(map)) = (kept, node.as_object_mut()) {
            let value = if value.ends_with('\n') {
                value.clone()
            } else {
                format!("{value}\n")
            };
            map.insert("format".into(), format.as_str().into());
            map.insert("value".into(), value.into());
            continue;
        }
        if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
            restore(children, raw);
        }
    }
}
