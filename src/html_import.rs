//! HTML5-to-Carve migration boundary.

use crate::ast::*;
use crate::render::{semantic_value_target, EXTENDED_SEMANTIC_SPAN_ORDER};
use crate::{render_carve, RenderDepthError};
use html5ever::tendril::TendrilSink;
use html5ever::{serialize, serialize::SerializeOpts};
use markup5ever_rcdom::{Handle, NodeData, RcDom, SerializableHandle};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtmlImportMode {
    Safe,
    Semantic,
    Roundtrip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtmlImportAdapter {
    Generic,
    Tiptap,
    Prosemirror,
    Ckeditor,
    Tinymce,
    Word,
    GoogleDocs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtmlImportSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtmlImportDiagnosticCode {
    ElementDropped,
    ElementUnwrapped,
    AttributeDropped,
    StyleUnmapped,
    TableDegraded,
    RawPreserved,
    DiagnosticsTruncated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlImportDiagnostic {
    pub code: HtmlImportDiagnosticCode,
    pub message: String,
    pub severity: HtmlImportSeverity,
    pub path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HtmlImportOptions {
    pub mode: HtmlImportMode,
    pub adapter: HtmlImportAdapter,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_diagnostics: usize,
}

impl Default for HtmlImportOptions {
    fn default() -> Self {
        Self {
            mode: HtmlImportMode::Safe,
            adapter: HtmlImportAdapter::Generic,
            max_depth: 128,
            max_nodes: 1_000_000,
            max_diagnostics: 1_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlImportReport {
    pub mode: HtmlImportMode,
    pub adapter: HtmlImportAdapter,
    pub diagnostics: Vec<HtmlImportDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlImportResult<T> {
    pub value: T,
    pub report: HtmlImportReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtmlImportError {
    DepthLimit,
    NodeLimit,
    RenderDepth,
}

struct Importer<'a> {
    opts: &'a HtmlImportOptions,
    diagnostics: Vec<HtmlImportDiagnostic>,
    nodes: usize,
}

impl<'a> Importer<'a> {
    fn enter(&mut self, depth: usize) -> Result<(), HtmlImportError> {
        if depth > self.opts.max_depth {
            return Err(HtmlImportError::DepthLimit);
        }
        self.nodes += 1;
        if self.nodes > self.opts.max_nodes {
            return Err(HtmlImportError::NodeLimit);
        }
        Ok(())
    }
    fn diag(
        &mut self,
        code: HtmlImportDiagnosticCode,
        message: String,
        severity: HtmlImportSeverity,
        path: &str,
    ) {
        if self.diagnostics.len() >= self.opts.max_diagnostics {
            if let Some(last) = self.diagnostics.last_mut() {
                *last = HtmlImportDiagnostic {
                    code: HtmlImportDiagnosticCode::DiagnosticsTruncated,
                    message: "HTML import diagnostics limit reached".into(),
                    severity: HtmlImportSeverity::Error,
                    path: None,
                };
            }
            return;
        }
        self.diagnostics.push(HtmlImportDiagnostic {
            code,
            message,
            severity,
            path: Some(path.into()),
        });
    }
    fn is_block_tag(tag: &str) -> bool {
        matches!(
            tag,
            "html"
                | "head"
                | "body"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "p"
                | "blockquote"
                | "ul"
                | "ol"
                | "pre"
                | "hr"
                | "table"
                | "div"
                | "section"
                | "article"
                | "main"
                | "nav"
                | "header"
                | "footer"
        )
    }

    fn tag(handle: &Handle) -> Option<String> {
        match &handle.data {
            NodeData::Element { name, .. } => Some(name.local.to_string()),
            _ => None,
        }
    }
    fn attr(handle: &Handle, wanted: &str) -> Option<String> {
        match &handle.data {
            NodeData::Element { attrs, .. } => attrs
                .borrow()
                .iter()
                .find(|a| a.name.local.as_ref() == wanted)
                .map(|a| a.value.to_string()),
            _ => None,
        }
    }
    fn text(handle: &Handle) -> String {
        match &handle.data {
            NodeData::Text { contents } => contents.borrow().to_string(),
            _ => handle.children.borrow().iter().map(Self::text).collect(),
        }
    }
    fn html(handle: &Handle) -> String {
        let mut bytes = Vec::new();
        let serializable = SerializableHandle::from(handle.clone());
        if serialize(&mut bytes, &serializable, SerializeOpts::default()).is_err() {
            return String::new();
        }
        let inner = String::from_utf8(bytes).unwrap_or_default();
        let NodeData::Element { name, attrs, .. } = &handle.data else {
            return inner;
        };
        let tag = name.local.as_ref();
        let attributes = attrs
            .borrow()
            .iter()
            .map(|attribute| {
                let value = attribute
                    .value
                    .replace('&', "&amp;")
                    .replace('"', "&quot;")
                    .replace('<', "&lt;");
                format!(" {}=\"{}\"", attribute.name.local, value)
            })
            .collect::<String>();
        if matches!(
            tag,
            "area"
                | "base"
                | "br"
                | "col"
                | "embed"
                | "hr"
                | "img"
                | "input"
                | "link"
                | "meta"
                | "param"
                | "source"
                | "track"
                | "wbr"
        ) {
            format!("<{tag}{attributes}>")
        } else {
            format!("<{tag}{attributes}>{inner}</{tag}>")
        }
    }
    fn attrs(&mut self, handle: &Handle, path: &str) -> Option<Attrs> {
        let tag = Self::tag(handle).unwrap_or_default();
        let mut out = Attrs::default();
        if let NodeData::Element { attrs, .. } = &handle.data {
            for attr in attrs.borrow().iter() {
                let name = attr.name.local.to_string();
                let value = attr.value.to_string();
                if name.starts_with("on") {
                    self.diag(
                        HtmlImportDiagnosticCode::AttributeDropped,
                        format!("Dropped event-handler attribute {name} on <{tag}>"),
                        HtmlImportSeverity::Warning,
                        path,
                    );
                } else if name == "id" {
                    out.id = Some(value);
                } else if name == "class" {
                    out.classes
                        .extend(value.split_whitespace().map(str::to_owned));
                } else if is_semantic_span_tag(&tag)
                    && semantic_value_target(&tag) == Some(name.as_str())
                {
                    // The VALUE of a compact semantic span (PART 9 §10): the
                    // `title` on an `<abbr>`, the `datetime` on a `<time>`.
                    // `inline` reads it straight off the element and writes it
                    // as the attribute's value, so keeping it here as well
                    // would spell the same string twice - and diagnosing it as
                    // dropped, which is where `datetime` used to land, would
                    // report a loss that no longer happens (carve#1140).
                } else if (name.starts_with("data-")
                    && name != "data-djot-src"
                    && name != "data-carve-src")
                    || (name == "title" && tag != "a" && tag != "img")
                    // A header cell's `scope` is REPRESENTABLE (PART 10 SST9)
                    // and is kept here, so a value the positional default
                    // cannot explain survives the import. `import_table` drops
                    // it again when it merely restates that default, which is
                    // what stops this engine's own output being read back as
                    // if the author had typed it (carve-rs#944).
                    || (name == "scope" && (tag == "th" || tag == "td"))
                {
                    out.key_values.insert(name, value);
                } else if name == "style" {
                    self.diag(
                        HtmlImportDiagnosticCode::StyleUnmapped,
                        "CSS declarations were not mapped".into(),
                        HtmlImportSeverity::Info,
                        path,
                    );
                } else if !matches!(
                    (tag.as_str(), name.as_str()),
                    ("a", "href")
                        | ("img", "src" | "alt")
                        | ("ol", "start" | "type")
                        | ("td" | "th", "rowspan" | "colspan")
                ) {
                    self.diag(
                        HtmlImportDiagnosticCode::AttributeDropped,
                        format!("Dropped unsupported attribute {name} on <{tag}>"),
                        HtmlImportSeverity::Info,
                        path,
                    );
                }
            }
        }
        if out == Attrs::default() {
            None
        } else {
            Some(out)
        }
    }
    fn blocks(
        &mut self,
        handles: &[Handle],
        parent: &str,
        depth: usize,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let mut out = Vec::new();
        let mut inline = Vec::new();
        for (i, handle) in handles.iter().enumerate() {
            let tag = Self::tag(handle);
            let is_block = tag.as_deref().map(Self::is_block_tag).unwrap_or(false);
            if is_block {
                if !inline.is_empty() {
                    let children = self.inlines(&inline, parent, depth + 1)?;
                    if visible(&children) {
                        out.push(BlockNode::Paragraph(Paragraph {
                            attrs: None,
                            children,
                            at_content_column: true,
                            pos: None,
                        }));
                    }
                    inline.clear();
                }
                let path = format!("{parent}/{}[{}]", tag.unwrap(), i + 1);
                out.extend(self.block(handle, &path, depth + 1)?);
            } else {
                inline.push(handle.clone());
            }
        }
        if !inline.is_empty() {
            let children = self.inlines(&inline, parent, depth + 1)?;
            if visible(&children) {
                out.push(BlockNode::Paragraph(Paragraph {
                    attrs: None,
                    children,
                    at_content_column: true,
                    pos: None,
                }));
            }
        }
        Ok(out)
    }
    fn block(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        self.enter(depth)?;
        let tag = Self::tag(h).unwrap();
        let attrs = self.attrs(h, path);
        let children = h.children.borrow();
        if tag == "html" || tag == "head" || tag == "body" {
            return self.blocks(&children, path, depth + 1);
        }
        if matches!(tag.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
            return Ok(vec![BlockNode::Heading(Heading {
                attrs,
                level: tag[1..].parse().unwrap(),
                children: self.inlines(&children, path, depth + 1)?,
                pos: None,
            })]);
        }
        if tag == "p" {
            return Ok(vec![BlockNode::Paragraph(Paragraph {
                attrs,
                children: self.inlines(&children, path, depth + 1)?,
                at_content_column: true,
                pos: None,
            })]);
        }
        if tag == "blockquote" {
            return Ok(vec![BlockNode::BlockQuote(BlockQuote {
                attrs,
                children: self.blocks(&children, path, depth + 1)?,
                pos: None,
            })]);
        }
        if tag == "pre" {
            let code = children
                .iter()
                .find(|n| Self::tag(n).as_deref() == Some("code"))
                .unwrap_or(h);
            let class = Self::attr(code, "class").unwrap_or_default();
            let lang = class
                .split_whitespace()
                .find_map(|c| c.strip_prefix("language-").map(str::to_owned));
            return Ok(vec![BlockNode::CodeBlock(CodeBlock {
                attrs,
                lang,
                title: None,
                label: None,
                content: Self::text(code),
                pos: None,
            })]);
        }
        if tag == "hr" {
            return Ok(vec![BlockNode::ThematicBreak(ThematicBreak {
                marker: None,
                attrs,
                pos: None,
            })]);
        }
        if tag == "ul" || tag == "ol" {
            let ordered = tag == "ol";
            let mut items = Vec::new();
            for (i, li) in children
                .iter()
                .filter(|n| Self::tag(n).as_deref() == Some("li"))
                .enumerate()
            {
                let p = format!("{path}/li[{}]", i + 1);
                items.push(ListItem {
                    attrs: self.attrs(li, &p),
                    checked: None,
                    children: self.blocks(&li.children.borrow(), &p, depth + 1)?,
                    pos: None,
                });
            }
            let start = if ordered {
                Self::attr(h, "start")
                    .and_then(|s| s.parse().ok())
                    .filter(|v| *v != 1)
            } else {
                None
            };
            return Ok(vec![BlockNode::List(List {
                attrs,
                ordered,
                start,
                ol_type: None,
                bare_marker: false,
                delim: None,
                bullet_char: None,
                tight: false,
                items,
                pos: None,
            })]);
        }
        if tag == "table" {
            return Ok(vec![BlockNode::Table(self.table(h, path, depth, attrs)?)]);
        }
        if tag == "div" {
            return Ok(vec![BlockNode::Div(Div {
                attrs,
                label: None,
                children: self.blocks(&children, path, depth + 1)?,
                pos: None,
            })]);
        }
        if self.opts.mode == HtmlImportMode::Roundtrip {
            self.diag(
                HtmlImportDiagnosticCode::RawPreserved,
                format!("Preserved unsupported <{tag}> element as raw HTML"),
                HtmlImportSeverity::Warning,
                path,
            );
            return Ok(vec![BlockNode::RawBlock(RawBlock {
                format: "html".into(),
                content: Self::html(h),
                pos: None,
            })]);
        }
        self.diag(
            HtmlImportDiagnosticCode::ElementUnwrapped,
            format!("Unwrapped unsupported <{tag}> element"),
            HtmlImportSeverity::Info,
            path,
        );
        self.blocks(&children, path, depth + 1)
    }
    fn table(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
    ) -> Result<Table, HtmlImportError> {
        fn rows(h: &Handle, out: &mut Vec<Handle>) {
            if Importer::tag(h).as_deref() == Some("tr") {
                out.push(h.clone());
            } else {
                for c in h.children.borrow().iter() {
                    rows(c, out);
                }
            }
        }
        let mut trs = Vec::new();
        rows(h, &mut trs);
        let mut result = Vec::new();
        for (r, tr) in trs.iter().enumerate() {
            let mut cells = Vec::new();
            for (c, cell) in tr
                .children
                .borrow()
                .iter()
                .filter(|n| matches!(Self::tag(n).as_deref(), Some("td" | "th")))
                .enumerate()
            {
                let p = format!(
                    "{path}/tr[{}]/{}[{}]",
                    r + 1,
                    Self::tag(cell).unwrap(),
                    c + 1
                );
                if Self::attr(cell, "rowspan").as_deref().unwrap_or("1") != "1"
                    || Self::attr(cell, "colspan").as_deref().unwrap_or("1") != "1"
                {
                    self.diag(
                        HtmlImportDiagnosticCode::TableDegraded,
                        "Table spans were flattened by this importer".into(),
                        HtmlImportSeverity::Warning,
                        &p,
                    );
                }
                cells.push(TableCell {
                    header: Self::tag(cell).as_deref() == Some("th"),
                    span: None,
                    align: None,
                    attrs: self.attrs(cell, &p),
                    children: self.inlines(&cell.children.borrow(), &p, depth + 1)?,
                    pos: None,
                });
            }
            result.push(TableRow {
                cells,
                attrs: None,
                pos: None,
            });
        }

        // PART 10 SST9 gives every `th` a `scope` from its POSITION: `col` in
        // the leading run of all-header rows, `row` for a header cell below it.
        // A value equal to that default carries no information the renderer
        // cannot reproduce, and importing it would write this engine's own
        // output back as if the author had written it. A value the default
        // cannot explain - `colgroup`, `rowgroup`, which have no marker
        // spelling - is the only way to get it, so it stays (carve-rs#944).
        let head_run = result
            .iter()
            .take_while(|row| !row.cells.is_empty() && row.cells.iter().all(|c| c.header))
            .count();
        for (r, row) in result.iter_mut().enumerate() {
            for cell in row.cells.iter_mut() {
                if !cell.header {
                    continue;
                }
                let default = if r < head_run { "col" } else { "row" };
                let Some(attrs) = cell.attrs.as_mut() else {
                    continue;
                };
                if attrs.key_values.get("scope").map(String::as_str) == Some(default) {
                    attrs.key_values.remove("scope");
                }
            }
        }
        Ok(Table {
            attrs,
            caption: None,
            short_caption: None,
            rows: result,
            pos: None,
        })
    }
    fn inlines(
        &mut self,
        handles: &[Handle],
        parent: &str,
        depth: usize,
    ) -> Result<Vec<InlineNode>, HtmlImportError> {
        let mut out = Vec::new();
        for (i, h) in handles.iter().enumerate() {
            let tag = Self::tag(h).unwrap_or_else(|| "text()".into());
            let path = format!("{parent}/{tag}[{}]", i + 1);
            out.extend(self.inline(h, &path, depth)?);
        }
        Ok(coalesce(out))
    }
    fn inline(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
    ) -> Result<Vec<InlineNode>, HtmlImportError> {
        self.enter(depth)?;
        if let NodeData::Text { contents } = &h.data {
            return Ok(vec![InlineNode::text(collapse(&contents.borrow()))]);
        }
        let Some(tag) = Self::tag(h) else {
            return Ok(Vec::new());
        };
        if matches!(tag.as_str(), "script" | "style" | "template" | "noscript") {
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                format!("Dropped active <{tag}> element"),
                HtmlImportSeverity::Warning,
                path,
            );
            return Ok(Vec::new());
        }
        let attrs = self.attrs(h, path);
        let children = self.inlines(&h.children.borrow(), path, depth + 1)?;
        let emphasis = |kind| {
            InlineNode::Emphasis(Emphasis {
                attrs: attrs.clone(),
                kind,
                children: children.clone(),
                pos: None,
            })
        };
        let node = match tag.as_str() {
            "em" | "i" => emphasis(EmphasisKind::Italic),
            "strong" | "b" => emphasis(EmphasisKind::Strong),
            "del" | "s" | "strike" => emphasis(EmphasisKind::Strike),
            "u" => emphasis(EmphasisKind::Underline),
            "mark" => emphasis(EmphasisKind::Highlight),
            "sub" => emphasis(EmphasisKind::Sub),
            "sup" => emphasis(EmphasisKind::Super),
            "code" => InlineNode::code(Self::text(h), attrs),
            "a" => InlineNode::Link(Link {
                attrs,
                href: Self::attr(h, "href").unwrap_or_default(),
                title: Self::attr(h, "title"),
                children,
                ref_label: None,
                raw_ref: None,
                from_crossref: false,
                from_heading_reference: false,
                pos: None,
            }),
            "img" => InlineNode::Image(Image {
                attrs,
                src: Self::attr(h, "src").unwrap_or_default(),
                alt: Self::attr(h, "alt").unwrap_or_default(),
                title: Self::attr(h, "title"),
                ref_label: None,
                raw_ref: None,
                pos: None,
            }),
            "br" => InlineNode::hard_break(),
            "span" if attrs.is_some() => InlineNode::Span(Span {
                attrs,
                children,
                injected: false,
                pos: None,
            }),
            // PART 9 §10, carve#1140. The seven names the compact span
            // attribute spells exactly. They belong here, with the other
            // elements Carve can express, rather than behind a mode branch:
            // `roundtrip` raw-preserves only what Carve CANNOT express, so
            // placing them here maps them in all three modes by construction.
            name if is_semantic_span_tag(name) => {
                let mut attrs = attrs.unwrap_or_default();
                // A name that carries no value, or an element that omits the
                // attribute it would carry it in, gives the bare boolean.
                let value = semantic_value_target(name)
                    .and_then(|source| Self::attr(h, source))
                    .unwrap_or_default();
                attrs.key_values.insert(name.to_string(), value);
                InlineNode::Span(Span {
                    attrs: Some(attrs),
                    children,
                    injected: false,
                    pos: None,
                })
            }
            _ if self.opts.mode == HtmlImportMode::Roundtrip => {
                self.diag(
                    HtmlImportDiagnosticCode::RawPreserved,
                    format!("Preserved unsupported <{tag}> element as raw HTML"),
                    HtmlImportSeverity::Warning,
                    path,
                );
                InlineNode::RawInline(RawInline {
                    format: "html".into(),
                    content: Self::html(h),
                    injected: false,
                    pos: None,
                })
            }
            _ => {
                self.diag(
                    HtmlImportDiagnosticCode::ElementUnwrapped,
                    format!("Unwrapped unsupported <{tag}> element"),
                    HtmlImportSeverity::Info,
                    path,
                );
                return Ok(children);
            }
        };
        Ok(vec![node])
    }
}

/// Whether an HTML element name is one of the seven PART 9 §10 spells as a
/// compact span attribute.
///
/// The list and the value mapping are the renderer's, read rather than
/// repeated: a name that joins or leaves the set, or starts carrying its value
/// somewhere else, cannot be right in the renderer and stale in the importer.
fn is_semantic_span_tag(tag: &str) -> bool {
    EXTENDED_SEMANTIC_SPAN_ORDER.contains(&tag)
}

fn collapse(s: &str) -> String {
    let value = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() {
        return if s.is_empty() {
            String::new()
        } else {
            " ".into()
        };
    }
    format!(
        "{}{}{}",
        if s.chars().next().is_some_and(char::is_whitespace) {
            " "
        } else {
            ""
        },
        value,
        if s.chars().last().is_some_and(char::is_whitespace) {
            " "
        } else {
            ""
        }
    )
}
fn visible(nodes: &[InlineNode]) -> bool {
    nodes
        .iter()
        .any(|n| !matches!(n, InlineNode::Text(t) if t.value.trim().is_empty()))
}
fn coalesce(nodes: Vec<InlineNode>) -> Vec<InlineNode> {
    let mut out = Vec::new();
    for n in nodes {
        if let (Some(InlineNode::Text(last)), InlineNode::Text(next)) = (out.last_mut(), &n) {
            last.value.push_str(&next.value);
        } else {
            out.push(n);
        }
    }
    out
}

pub fn html_to_ast(
    html: &str,
    options: &HtmlImportOptions,
) -> Result<HtmlImportResult<Document>, HtmlImportError> {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let mut importer = Importer {
        opts: options,
        diagnostics: Vec::new(),
        nodes: 0,
    };
    let children = importer.blocks(&dom.document.children.borrow(), "", 0)?;
    Ok(HtmlImportResult {
        value: Document {
            frontmatter: BTreeMap::new(),
            frontmatter_raw: None,
            footnote_defs: BTreeMap::new(),
            footnote_def_pos: BTreeMap::new(),
            children,
            source_len: 0,
            ingest_payload_len: 0,
        },
        report: HtmlImportReport {
            mode: options.mode,
            adapter: options.adapter,
            diagnostics: importer.diagnostics,
        },
    })
}

pub fn html_to_carve(
    html: &str,
    options: &HtmlImportOptions,
) -> Result<HtmlImportResult<String>, HtmlImportError> {
    let result = html_to_ast(html, options)?;
    let value =
        render_carve(&result.value).map_err(|_: RenderDepthError| HtmlImportError::RenderDepth)?;
    Ok(HtmlImportResult {
        value,
        report: result.report,
    })
}
