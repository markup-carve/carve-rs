//! HTML5-to-Carve migration boundary.

use crate::ast::*;
use crate::render::{semantic_value_target, EXTENDED_SEMANTIC_SPAN_ORDER};
use crate::{render_carve, RenderDepthError};
use html5ever::tendril::TendrilSink;
use html5ever::{serialize, serialize::SerializeOpts};
use markup5ever_rcdom::{Handle, NodeData, RcDom, SerializableHandle};
use std::collections::{BTreeMap, BTreeSet};

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
    /// A structure the AST holds and Carve 0.1 SOURCE has no spelling for, so
    /// only a WRITER loses it (PART 12 §16). Reported by `html_to_carve`;
    /// `html_to_ast` keeps the structure and says nothing.
    StructureUnspellable,
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
    /// How many `<q>` elements are open around the node being read. HTML5
    /// leaves the marks to the user agent and every one of them alternates, so
    /// the depth is what chooses between the double and the single pair.
    quote_depth: usize,
    /// The losses a WRITER takes, held back until one writes (PART 12 §16).
    unspellable: Vec<(String, String)>,
}

/// The largest position a Roman marker is written for: `MMMCMXCIX`, the end of
/// the classic range. Past it the additive form grows without bound - the
/// marker for position `n` carries `n / 1000` copies of `m` - and `start` is an
/// author-supplied integer, so the cap is what stops a twenty-byte attribute
/// from buying an arbitrarily large marker once per item.
const MAX_ROMAN_MARKER: usize = 3999;

/// HTML's own maxima for the two span attributes, and the largest value read as
/// a number at all before it defaults.
const MAX_COLSPAN: usize = 1000;
const MAX_ROWSPAN: usize = 65534;
const MAX_SAFE_SPAN: u64 = 9_007_199_254_740_991;

/// One imported cell, with the two spans resolved but not yet laid out.
struct BuiltCell {
    cell: TableCell,
    colspan: usize,
    rowspan: usize,
}

/// A table's `<thead>` / `<tbody>` / `<tfoot>`, collected on the way THROUGH it
/// rather than read back off its rows.
///
/// The two vectors are parallel and a row's section id indexes both. Deriving
/// the list from the rows was the shape that missed a section with no rows -
/// `<tbody id="empty"></tbody>` is one of the table's sections and its
/// attributes are as lost as any other's, but no row points at it.
struct Sections {
    /// The section's tag name, which is what says whether it has a slot at all.
    tags: Vec<String>,
    /// The section's own attributes and the path to report them at. Taken by
    /// `row_groups` when it places them on a body group, so what is left is
    /// exactly what nothing holds.
    attrs: Vec<Option<(Attrs, String)>>,
}

/// The `<annotation>` encodings that DECLARE their payload to be TeX.
///
/// Matched case-insensitively against the WHOLE value, never as a substring.
/// A substring test for `tex` accepts every `text/*` encoding there is - the
/// word `text` contains it - so `<annotation encoding="text/plain">` would
/// have been read as an equation.
const TEX_ANNOTATION_ENCODINGS: [&str; 3] = ["application/x-tex", "text/x-tex", "latex"];

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
                | "dl"
                | "pre"
                | "hr"
                | "table"
                | "details"
                | "div"
                | "section"
                | "article"
                | "main"
                | "nav"
                | "header"
                | "footer"
                | "figure"
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
    /// Tiers 1 and 2 of carve#1210's D6: the TeX the producer already put in
    /// the element, and which tier supplied it.
    ///
    /// 1. an `<annotation>` whose `encoding` DECLARES TeX;
    /// 2. else the `alttext` attribute, whose contents MathML does not declare
    ///    - hence the tier, and hence the `info` the caller reports for it.
    ///
    /// Annotation first, and the order carries the ruling: where a declared
    /// encoding and an undeclared attribute disagree, the declared one wins.
    ///
    /// The body is taken as written, `{\displaystyle …}` wrapper and all -
    /// Carve math content is opaque TeX and rewriting it would be a second
    /// decision. The whitespace AROUND it is not part of the equation and is
    /// trimmed, because keeping it builds a node this engine cannot write and
    /// read back: `$`\n  x^2\n`$` returns from its own writer as `\nx^2\n`,
    /// the indentation gone, which is exactly the shape a pretty-printed
    /// annotation produces.
    ///
    /// `None` is tier 3, whose two answers are the caller's.
    fn math_tex(h: &Handle) -> Option<(u8, String)> {
        if let Some(annotated) = Self::tex_annotation(h) {
            return Some((1, annotated));
        }
        let alttext = Self::attr(h, "alttext")?;
        let trimmed = alttext.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some((2, trimmed.to_string()))
    }
    /// The `<annotation>` a `<semantics>` carries, if it declares TeX.
    ///
    /// Both hops are DIRECT children - the `<semantics>` of the `<math>`, the
    /// annotation of that `<semantics>`. A recursive search reaches the
    /// `<annotation>` nested inside an `<annotation-xml>` payload, which
    /// describes the equation in another language and is not a presentation of
    /// the outer element at all.
    ///
    /// An annotation that declares TeX and then holds nothing does not settle
    /// the tier: the search continues, because a later sibling may hold the
    /// equation and stopping at the empty one would answer with the wrong tier.
    fn tex_annotation(h: &Handle) -> Option<String> {
        for semantics in h.children.borrow().iter() {
            if Self::tag(semantics).as_deref() != Some("semantics") {
                continue;
            }
            for annotation in semantics.children.borrow().iter() {
                if Self::tag(annotation).as_deref() != Some("annotation") {
                    continue;
                }
                let Some(encoding) = Self::attr(annotation, "encoding") else {
                    continue;
                };
                if !TEX_ANNOTATION_ENCODINGS
                    .contains(&encoding.trim().to_ascii_lowercase().as_str())
                {
                    continue;
                }
                let text = Self::flat_text(annotation);
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }
    /// `text()` without its recursion, for the annotation - which is read to
    /// settle the tier, so it is read before any depth counter has seen it. At
    /// a caller-raised `max_depth` a recursive read is the thing that fails
    /// first, and a blown stack is not the typed error this API promises.
    fn flat_text(h: &Handle) -> String {
        let mut out = String::new();
        let mut pending = vec![h.clone()];
        while let Some(current) = pending.pop() {
            if let NodeData::Text { contents } = &current.data {
                out.push_str(&contents.borrow());
            }
            let children = current.children.borrow();
            for child in children.iter().rev() {
                pending.push(child.clone());
            }
        }
        out
    }
    /// Charge a subtree the import will not walk against the budget one it
    /// walks would pay, and check its depth on the way.
    ///
    /// Explicit stack rather than recursion: this runs on input the DOM parser
    /// accepted at any depth, and its whole point is to reach `max_depth`
    /// before something that recurses does.
    fn charge_subtree(&mut self, h: &Handle, depth: usize) -> Result<(), HtmlImportError> {
        let mut pending = vec![(h.clone(), depth)];
        while let Some((current, current_depth)) = pending.pop() {
            for child in current.children.borrow().iter() {
                self.enter(current_depth + 1)?;
                pending.push((child.clone(), current_depth + 1));
            }
        }
        Ok(())
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
                    // `open` on a `<details>` is REPRESENTABLE: the details
                    // extension reads it off the admonition's attributes and
                    // puts it back on the tag, and Carve spells a valueless
                    // attribute `{open}`. Dropping it would import a disclosure
                    // that starts open as one that starts closed.
                    || (name == "open" && tag == "details")
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
                        // READ by the math branch and carried to the node, so
                        // reporting them dropped would name a loss that does
                        // not happen. `xmlns` is the namespace declaration that
                        // makes the element MathML in the first place: consumed
                        // by having been recognized, not discarded.
                        | ("math", "display" | "alttext" | "xmlns")
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
    /// The NAMES of the attributes `attrs` kept, for a report that says which
    /// ones were lost rather than that some were.
    fn attr_names(attrs: &Attrs) -> Vec<String> {
        attrs
            .id
            .iter()
            .map(|_| "id".to_owned())
            .chain((!attrs.classes.is_empty()).then(|| "class".to_owned()))
            .chain(attrs.key_values.keys().cloned())
            .collect()
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
            let mut attrs = attrs;
            let ol_type = if ordered {
                self.ordered_list_type(h, start.unwrap_or(1), items.len(), &mut attrs, path)
            } else {
                None
            };
            return Ok(vec![BlockNode::List(List {
                attrs,
                ordered,
                start,
                ol_type,
                bare_marker: false,
                delim: None,
                bullet_char: None,
                tight: false,
                items,
                pos: None,
            })]);
        }
        if tag == "details" {
            return Ok(vec![BlockNode::Admonition(
                self.details(h, path, depth, attrs)?,
            )]);
        }
        if tag == "dl" {
            return self.definition_list(h, path, depth, attrs);
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
        // This engine's own composite-figure shape (PART 9 §4c) comes back as
        // the node it left as. Only the exact own-output classes take these
        // paths; any other <figure> stays an unsupported element below.
        if tag == "figure" && Self::first_class(h).as_deref() == Some("carve-figure-group") {
            return self.figure_group(h, path, depth, attrs);
        }
        if tag == "figure" && Self::first_class(h).as_deref() == Some("carve-figure-panel") {
            return self.figure_panel(h, path, depth, attrs);
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
    /// `<details>/<summary>` to a `details` admonition.
    ///
    /// Before this the element unwrapped: the summary and the body were flushed
    /// into the same inline run, so a disclosure widget imported as one
    /// paragraph whose first words were the summary, with nothing separating
    /// them - and three `element-unwrapped` diagnostics that named the elements
    /// without saying the document had lost a section.
    ///
    /// `::: details` is the shape Carve already has for this, and the bundled
    /// details extension renders it straight back to `<details>/<summary>`, so
    /// the round trip closes. A generic `<div class="details">` would not: it
    /// renders the summary as ordinary body text.
    fn details(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
    ) -> Result<Admonition, HtmlImportError> {
        let children = h.children.borrow();
        // HTML5 puts the summary first, but takes it wherever it is; the rest
        // of the element is the body in document order either way. A second
        // `<summary>` is not one, and falls through to the block walk, where it
        // is reported like any other element Carve cannot express.
        let summary = children
            .iter()
            .position(|c| Self::tag(c).as_deref() == Some("summary"));
        let title = match summary {
            Some(i) => {
                let p = format!("{path}/summary[{}]", i + 1);
                // The summary's OWN attributes are read, not just its children.
                // Skipping them would take an `onclick` off the element without
                // a word, which is the silent shape this whole tracker exists
                // to remove. A span is where an id or a class lands, since the
                // title is inline content and the admonition's attribute slot
                // belongs to the `<details>` tag.
                let title_attrs = self.attrs(&children[i], &p);
                let inlines = self.inlines(&children[i].children.borrow(), &p, depth + 1)?;
                Some(match title_attrs {
                    Some(attrs) => vec![InlineNode::Span(Span {
                        attrs: Some(attrs),
                        children: inlines,
                        injected: false,
                        pos: None,
                    })],
                    None => inlines,
                })
            }
            None => None,
        };
        let body: Vec<Handle> = children
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != summary)
            .map(|(_, c)| c.clone())
            .collect();
        Ok(Admonition {
            attrs,
            kind: "details".into(),
            title,
            label: None,
            children: self.blocks(&body, path, depth + 1)?,
            pos: None,
        })
    }
    /// The numbering style this `<ol>` is written with, or `None` for decimal.
    ///
    /// `type` was on the list of attributes the importer does not report, which
    /// read as "handled" and was not: nothing ever set `List::ol_type`, so an
    /// `<ol type="a">` imported as a decimal list and the style left the
    /// document without a word. Carve spells all four styles in the MARKER, so
    /// the marker is where the style goes - and being in the marker it works at
    /// any depth, where an attribute block on a nested list would not.
    ///
    /// `None` is also the answer for the shapes whose markers this engine's own
    /// parser reads back as a different list, because a marker that re-parses
    /// wrong is worse than an attribute that at least renders the right `<ol>`.
    /// The three shapes were measured against `parse` rather than reasoned
    /// about, since the overlap between one-letter alphabetic markers and Roman
    /// numerals is resolved by the parser and not by a rule stated anywhere:
    ///
    /// - an alphabetic sequence that would run past `z` (`aa.` is not a marker);
    /// - a one-item alphabetic list at position 9, whose `i.` reads as Roman;
    /// - a one-item Roman list at 5, 10, 50, 100, 500 or 1000, whose `v.`,
    ///   `x.`, `l.`, `c.`, `d.` or `m.` reads as alphabetic. Position 1 is not
    ///   among them: the parser resolves a lone `i.` to Roman, which is what it
    ///   would have meant;
    /// - a Roman list running past `MMMCMXCIX`, where the additive form stops
    ///   being a numeral anyone reads and starts being a run of `m` whose
    ///   length is the start value over a thousand. That one is a resource
    ///   bound as much as a legibility one: `start` is an author-supplied
    ///   integer, so without it a twenty-byte attribute buys a marker of
    ///   arbitrary size, once per item.
    ///
    /// In those cases the raw `type` is KEPT as an attribute, which still
    /// renders the right `<ol>`, and the diagnostic says the style could not
    /// reach the marker. Before this it was dropped in silence.
    fn ordered_list_type(
        &mut self,
        h: &Handle,
        start: usize,
        items: usize,
        attrs: &mut Option<Attrs>,
        path: &str,
    ) -> Option<OrderedListType> {
        let value = Self::attr(h, "type")?;
        // `1` is the decimal default and the plain marker already means it.
        if value.is_empty() || value == "1" {
            return None;
        }
        let spelled = match value.as_str() {
            "a" => Some(OrderedListType::LowerAlpha),
            "A" => Some(OrderedListType::UpperAlpha),
            "i" => Some(OrderedListType::LowerRoman),
            "I" => Some(OrderedListType::UpperRoman),
            _ => None,
        };
        let last = start.saturating_add(items.saturating_sub(1));
        let representable = match spelled {
            Some(OrderedListType::LowerAlpha) | Some(OrderedListType::UpperAlpha) => {
                start >= 1 && last <= 26 && !(items == 1 && start == 9)
            }
            Some(OrderedListType::LowerRoman) | Some(OrderedListType::UpperRoman) => {
                start >= 1
                    && last <= MAX_ROMAN_MARKER
                    && !(items == 1 && matches!(start, 5 | 10 | 50 | 100 | 500 | 1000))
            }
            None => false,
        };
        if let (Some(ty), true) = (spelled, representable) {
            return Some(ty);
        }
        attrs
            .get_or_insert_with(Attrs::default)
            .key_values
            .insert("type".into(), value.clone());
        self.diag(
            HtmlImportDiagnosticCode::RawPreserved,
            format!(
                "Kept type=\"{value}\" as a raw attribute on <ol> instead of the marker: this list's markers would read back as a different list"
            ),
            HtmlImportSeverity::Info,
            path,
        );
        None
    }
    /// `<dl>` to `definition_list`.
    ///
    /// Before this the element had no branch at all, so it fell through to the
    /// unwrapping path: every term and every definition became inline content
    /// of one paragraph, and a two-entry glossary imported as a single run of
    /// words with no separator between a term and its own definition. The
    /// diagnostics said `element-unwrapped`, which is true of the element and
    /// says nothing about the document losing its structure.
    ///
    /// HTML5 gives `dl` two content models: `dt`/`dd` as direct children, or
    /// one `div` per group wrapping them. Both spell the same list, so the
    /// wrapper is unwrapped transparently - one level, which is the only level
    /// HTML5 allows. A `div` nested inside a wrapper is not a group; its terms
    /// stay unread rather than this importer inventing a flattening the source
    /// did not say.
    ///
    /// A group is a run of terms followed by a run of definitions, which is the
    /// same grouping the parser builds from `::` and `:` lines, so an imported
    /// list and a hand-written one produce the same tree.
    fn definition_list(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        // Each entry carries the depth of the element it came from, so a term
        // reached through a group wrapper descends from one level deeper than a
        // direct one. `enter` is called for every element this walk visits: the
        // wrapper is not a node in the result, but it is a level of nesting and
        // of recursion, and a limit a wrapper chain can step around is not a
        // limit.
        let mut entries: Vec<(String, Handle, usize)> = Vec::new();
        for (i, child) in h.children.borrow().iter().enumerate() {
            let Some(tag) = Self::tag(child) else {
                continue;
            };
            self.enter(depth + 1)?;
            match tag.as_str() {
                "dt" | "dd" => entries.push((tag, child.clone(), depth + 1)),
                "div" => {
                    let p = format!("{path}/div[{}]", i + 1);
                    // The wrapper groups rows for styling and carries nothing
                    // the `::` form spells, so it goes. Its attributes are read
                    // anyway: that is what reports a `style` or an event
                    // handler on it, and what makes an id or a class it does
                    // carry a stated loss rather than a silent one.
                    if self.attrs(child, &p).is_some() {
                        self.diag(
                            HtmlImportDiagnosticCode::AttributeDropped,
                            "Dropped the attributes of a <div> group wrapper in <dl>; a definition-list group has no slot for them".into(),
                            HtmlImportSeverity::Info,
                            &p,
                        );
                    }
                    for (j, wrapped) in child.children.borrow().iter().enumerate() {
                        let Some(inner) = Self::tag(wrapped) else {
                            continue;
                        };
                        self.enter(depth + 2)?;
                        if inner == "dt" || inner == "dd" {
                            entries.push((inner, wrapped.clone(), depth + 2));
                        } else {
                            // One level unwraps, which is the only level HTML5
                            // allows, so a `div` inside the wrapper is not a
                            // group. It is still reported: a doubly-wrapped
                            // list otherwise imports to nothing at all, which
                            // is the silent shape this row exists to remove.
                            self.dropped_in_dl(&inner, &format!("{p}/{inner}[{}]", j + 1));
                        }
                    }
                }
                other => self.dropped_in_dl(other, &format!("{path}/{other}[{}]", i + 1)),
            }
        }

        let mut before: Vec<BlockNode> = Vec::new();
        let mut items: Vec<DefinitionItem> = Vec::new();
        let mut terms: Vec<DefinitionTerm> = Vec::new();
        let mut definitions: Vec<DefinitionDef> = Vec::new();
        for (i, (tag, node, node_depth)) in entries.iter().enumerate() {
            let p = format!("{path}/{tag}[{}]", i + 1);
            if tag == "dt" {
                if !definitions.is_empty() {
                    items.push(DefinitionItem {
                        terms: std::mem::take(&mut terms),
                        definitions: std::mem::take(&mut definitions),
                        pos: None,
                    });
                }
                terms.push(DefinitionTerm {
                    attrs: self.attrs(node, &p),
                    children: self.inlines(&node.children.borrow(), &p, node_depth + 1)?,
                    pos: None,
                });
                continue;
            }
            if terms.is_empty() && definitions.is_empty() {
                // A `dd` before any `dt` is not valid HTML5, but a sliced-up
                // editor export produces one. It cannot become a group: a
                // definition line under an empty `::` re-parses as a paragraph,
                // so writing one would trade a silent loss for a corrupt
                // document. The content is emitted ahead of the list instead,
                // which keeps every word and stays valid Carve, and the
                // diagnostic states the role that did not survive.
                self.diag(
                    HtmlImportDiagnosticCode::ElementUnwrapped,
                    "A <dd> with no <dt> before it kept its content but not its role: it is emitted as blocks ahead of the definition list".into(),
                    HtmlImportSeverity::Warning,
                    &p,
                );
                before.extend(self.blocks(&node.children.borrow(), &p, node_depth + 1)?);
                continue;
            }
            definitions.push(DefinitionDef {
                attrs: self.attrs(node, &p),
                children: self.blocks(&node.children.borrow(), &p, node_depth + 1)?,
                pos: None,
            });
        }
        if !terms.is_empty() || !definitions.is_empty() {
            items.push(DefinitionItem {
                terms,
                definitions,
                pos: None,
            });
        }
        if items.is_empty() {
            return Ok(before);
        }
        before.push(BlockNode::DefinitionList(DefinitionList {
            attrs,
            items,
            pos: None,
        }));
        Ok(before)
    }
    fn dropped_in_dl(&mut self, tag: &str, path: &str) {
        self.diag(
            HtmlImportDiagnosticCode::ElementDropped,
            format!("Dropped <{tag}> inside <dl>: only <dt>, <dd> and a single <div> group wrapper are definition-list content"),
            HtmlImportSeverity::Warning,
            path,
        );
    }
    /// The element's FIRST class, the slot this engine's structural classes
    /// lead from (`carve-figure-group`, `carve-figure-panel`).
    fn first_class(handle: &Handle) -> Option<String> {
        Self::attr(handle, "class")?
            .split_whitespace()
            .next()
            .map(str::to_string)
    }

    /// Drop a structural class the renderer injected; what remains is what the
    /// author wrote. `None` when nothing else was carried.
    fn without_structural_class(attrs: Option<Attrs>, class: &str) -> Option<Attrs> {
        let mut attrs = attrs?;
        attrs.classes.retain(|c| c != class);
        if attrs.classes.is_empty() && attrs.id.is_none() && attrs.key_values.is_empty() {
            return None;
        }
        Some(attrs)
    }

    /// `<figure class="carve-figure-group">` back to the `figure_group` node
    /// it rendered from: the unconditional panels div unwraps into `children`
    /// (each panel routed back through [`Self::figure_panel`]), and the
    /// trailing `<figcaption>` is the group caption (PART 9 §4c).
    fn figure_group(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let attrs = Self::without_structural_class(attrs, "carve-figure-group");
        let mut children = Vec::new();
        let mut caption = None;
        for (i, child) in h.children.borrow().iter().enumerate() {
            match Self::tag(child).as_deref() {
                Some("div")
                    if Self::first_class(child).as_deref() == Some("carve-figure-panels") =>
                {
                    let p = format!("{path}/div[{}]", i + 1);
                    children = self.blocks(&child.children.borrow(), &p, depth + 1)?;
                }
                Some("figcaption") => {
                    let p = format!("{path}/figcaption[{}]", i + 1);
                    caption = Some(self.inlines(&child.children.borrow(), &p, depth + 1)?);
                }
                _ => {}
            }
        }
        Ok(vec![BlockNode::FigureGroup(FigureGroup {
            attrs,
            children,
            caption,
            pos: None,
        })])
    }

    /// `<figure class="carve-figure-panel">` back to the node it wrapped: the
    /// host plus its `<figcaption>` rebuild the `figure` the caption pass
    /// produced; a bare wrapped table (whose caption is its own `<caption>`)
    /// unwraps to the table node.
    fn figure_panel(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let attrs = Self::without_structural_class(attrs, "carve-figure-panel");
        let mut caption = None;
        let mut host = Vec::new();
        for child in h.children.borrow().iter() {
            if Self::tag(child).as_deref() == Some("figcaption") {
                caption = Some(self.inlines(&child.children.borrow(), path, depth + 1)?);
                continue;
            }
            // Pretty-printed margins between the wrapper and its host. Kept,
            // they lead the rebuilt image paragraph with a space, and the
            // writer's indented image line then re-parses as prose.
            if let NodeData::Text { contents } = &child.data {
                if contents.borrow().trim().is_empty() {
                    continue;
                }
            }
            host.push(child.clone());
        }
        let mut blocks = self.blocks(&host, path, depth + 1)?;
        let Some(caption) = caption else {
            return Ok(blocks);
        };
        if blocks.len() == 1 {
            let target = match blocks.remove(0) {
                // A sole image renders bare inside the panel, so it comes back
                // as a one-image paragraph; the figure the parser builds holds
                // the IMAGE as its target.
                BlockNode::Paragraph(p)
                    if p.children.len() == 1 && matches!(p.children[0], InlineNode::Image(_)) =>
                {
                    match p.children.into_iter().next() {
                        Some(InlineNode::Image(img)) => FigureTarget::Image(img),
                        _ => unreachable!("the match guard saw an image"),
                    }
                }
                BlockNode::Paragraph(p) => FigureTarget::Paragraph(p),
                BlockNode::BlockImage(img) => FigureTarget::Image(img),
                BlockNode::BlockQuote(quote) => FigureTarget::BlockQuote(quote),
                BlockNode::Table(table) => FigureTarget::Table(table),
                BlockNode::CodeBlock(code) => FigureTarget::CodeBlock(code),
                other => {
                    blocks.insert(0, other);
                    blocks.push(BlockNode::Paragraph(Paragraph {
                        attrs: None,
                        children: caption,
                        at_content_column: true,
                        pos: None,
                    }));
                    return Ok(blocks);
                }
            };
            return Ok(vec![BlockNode::Figure(Figure {
                attrs,
                target,
                caption,
                short_caption: None,
                pos: None,
            })]);
        }
        blocks.push(BlockNode::Paragraph(Paragraph {
            attrs: None,
            children: caption,
            at_content_column: true,
            pos: None,
        }));
        Ok(blocks)
    }

    /// A `colspan` / `rowspan` value, by HTML's rules: a non-negative integer,
    /// clamped to the attribute's own maximum, defaulting when it is not one.
    ///
    /// The clamp is not decoration. Each unit of a span becomes a CELL below, so
    /// an unclamped `colspan="1000000000"` is a 30-byte input asking for a
    /// billion of them; the generated cells are charged to `max_nodes` on top of
    /// this, so the two together bound what a table can cost.
    fn span_count(cell: &Handle, name: &str, max: usize, min: usize) -> usize {
        let Some(raw) = Self::attr(cell, name) else {
            return 1;
        };
        let raw = raw.trim();
        if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
            return 1;
        }
        // The same cutoff carve-js reaches through `Number.isSafeInteger`, named
        // rather than inherited, so a value past it defaults on every engine
        // instead of clamping on one and defaulting on another.
        match raw.parse::<u64>() {
            // Clamped in `u64` and converted after, so a value above `usize` on
            // a 32-bit target reaches the maximum rather than truncating to
            // something under it.
            Ok(value) if value <= MAX_SAFE_SPAN => value.clamp(min as u64, max as u64) as usize,
            _ => 1,
        }
    }

    /// The imported cells, laid out with the continuation cells Carve spells `^`
    /// (this cell continues the one above) and `<` (it continues the one to its
    /// left).
    ///
    /// The model already carried both - `TableCell::span` is in PART 12 and the
    /// HTML renderer derives `rowspan` / `colspan` from a run of them - and the
    /// import simply threw them away: a spanning cell was written as an ordinary
    /// one and the row came up short, so `<td colspan="2">` produced a 1-cell
    /// row under a 2-column header, with `table-degraded` as the only trace.
    ///
    /// The renderer resolves a continuation by POSITION IN THE ROW'S CELL ARRAY,
    /// not by grid column - `^` attaches to the nearest row above whose cell at
    /// the same index is not itself a continuation - so a carried span occupies
    /// ONE array slot however many columns it covers, and the cells after it in
    /// the row shift left by the rest. Placing the carried marks first and
    /// filling the row's own cells around them is what keeps those indexes
    /// aligned.
    fn span_grid(
        &mut self,
        built: Vec<Vec<BuiltCell>>,
        row_attrs: &[Option<Attrs>],
        path: &str,
        depth: usize,
    ) -> Result<Vec<TableRow>, HtmlImportError> {
        fn continuation(span: TableCellSpan, header: bool) -> TableCell {
            TableCell {
                header,
                span: Some(span),
                align: None,
                attrs: None,
                children: Vec::new(),
                pos: None,
            }
        }
        let mut carried: Vec<(usize, usize)> = Vec::new();
        let mut rows = Vec::with_capacity(built.len());
        for (r, source_cells) in built.into_iter().enumerate() {
            let marks: BTreeSet<usize> = carried.iter().map(|&(index, _)| index).collect();
            let mut cells: Vec<TableCell> = Vec::new();
            let mut opened: Vec<(usize, usize)> = Vec::new();
            for BuiltCell {
                cell,
                colspan,
                rowspan,
            } in source_cells
            {
                while marks.contains(&cells.len()) {
                    self.enter(depth)?;
                    cells.push(continuation(TableCellSpan::Rowspan, false));
                }
                // Every slot the cell occupies in THIS row, so a rowspan can
                // carry a mark into each of them.
                let mut covered = vec![cells.len()];
                let header = cell.header;
                cells.push(cell);
                for _ in 1..colspan {
                    while marks.contains(&cells.len()) {
                        self.enter(depth)?;
                        cells.push(continuation(TableCellSpan::Rowspan, false));
                    }
                    covered.push(cells.len());
                    self.enter(depth)?;
                    cells.push(continuation(TableCellSpan::Colspan, header));
                }
                // A cell spanning BOTH ways carries a mark for each column it
                // covers, not one for its origin. The renderer resolves a `^`
                // against the cell at the SAME INDEX above it, so a single mark
                // left the next rowspan in the row resolving against a column it
                // does not own: `<td colspan="2" rowspan="2">A</td>` beside
                // `<td rowspan="2">B</td>` wrote `| ^ |  | ^ |` over them, which
                // renders a cell the table does not have and reports inventing
                // it.
                if rowspan > 1 {
                    opened.extend(covered.into_iter().map(|index| (index, rowspan - 1)));
                }
            }
            // A mark past the end of this row's own cells. Placing it still costs
            // nothing - it is a cell the span already owns - but a GAP before it
            // does: the index has to be kept, and an empty cell there is one the
            // source did not have. Only that invention is reported.
            let furthest = marks.iter().next_back().copied();
            let mut invented = false;
            if let Some(furthest) = furthest {
                while cells.len() <= furthest {
                    self.enter(depth)?;
                    if marks.contains(&cells.len()) {
                        cells.push(continuation(TableCellSpan::Rowspan, false));
                    } else {
                        cells.push(TableCell {
                            header: false,
                            span: None,
                            align: None,
                            attrs: None,
                            children: Vec::new(),
                            pos: None,
                        });
                        invented = true;
                    }
                }
            }
            if invented {
                self.diag(
                    HtmlImportDiagnosticCode::TableDegraded,
                    "Filled a row that is shorter than the spans reaching into it, with a cell the source did not have".into(),
                    HtmlImportSeverity::Warning,
                    &format!("{path}/tr[{}]", r + 1),
                );
            }
            rows.push(TableRow {
                cells,
                // The `<tr>`'s own attributes, which `TableRow::attrs` has a
                // slot for and the writer spells on the closing pipe.
                attrs: row_attrs.get(r).cloned().flatten(),
                pos: None,
            });
            carried = carried
                .into_iter()
                .filter_map(|(index, left)| (left > 1).then_some((index, left - 1)))
                .chain(opened)
                .collect();
        }
        Ok(rows)
    }

    /// The `<colgroup>` children of a table, which carry COLUMN structure Carve
    /// has nowhere to put.
    ///
    /// Only `<colgroup>` is looked for, and a `<col>` is not. Every `<col>` is
    /// inside one after parsing - "in table" insertion mode answers a `col`
    /// start tag by inserting an implied `<colgroup>` first, so a run of bare
    /// `<col>`s arrives as one wrapper holding all of them - and reporting the
    /// wrapper covers its children the way one report covers a dropped subtree
    /// everywhere else. A `| "col"` arm here matched nothing on any input, which
    /// is the check that cannot fail (carve#755).
    fn column_groups(h: &Handle, path: &str) -> Vec<String> {
        h.children
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, c)| Self::tag(c).as_deref() == Some("colgroup"))
            .map(|(i, _)| format!("{path}/colgroup[{}]", i + 1))
            .collect()
    }

    fn table(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
    ) -> Result<Table, HtmlImportError> {
        // Each row remembers the `<thead>` / `<tbody>` / `<tfoot>` it is in, by
        // an id minted when the walk enters one. A rowspan stops at its ROW
        // GROUP in HTML, so the group is not bookkeeping here: it is what says
        // how far down a span reaches.
        //
        // The sections themselves are collected here too, on the way through,
        // rather than read back off the rows afterwards: a section with NO rows
        // is one of the table's sections as well, and deriving the list from the
        // rows leaves a `<tbody id="empty"></tbody>` unread and its attributes
        // unreported.
        fn rows(
            h: &Handle,
            path: &str,
            section: Option<usize>,
            section_tags: &mut Vec<String>,
            section_nodes: &mut Vec<(Handle, String)>,
            out: &mut Vec<(Handle, Option<usize>)>,
        ) {
            if Importer::tag(h).as_deref() == Some("tr") {
                out.push((h.clone(), section));
                return;
            }
            let own = match Importer::tag(h).as_deref() {
                Some(tag @ ("thead" | "tbody" | "tfoot")) => {
                    section_tags.push(tag.to_owned());
                    section_nodes.push((h.clone(), path.to_owned()));
                    Some(section_tags.len() - 1)
                }
                _ => section,
            };
            for (i, c) in h.children.borrow().iter().enumerate() {
                let tag = Importer::tag(c).unwrap_or_default();
                rows(
                    c,
                    &format!("{path}/{tag}[{}]", i + 1),
                    own,
                    section_tags,
                    section_nodes,
                    out,
                );
            }
        }
        // `<caption>` is a DIRECT child of the table and carries the table's own
        // caption, which `Table::caption` has a slot for and Carve spells `^ text`
        // after the rows. The row walk below looks only for `tr`, so before this
        // the element was skipped and the caption left the document silently -
        // pandoc emits exactly this shape for every captioned table.
        let captions: Vec<(usize, Handle)> = h
            .children
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, c)| Importer::tag(c).as_deref() == Some("caption"))
            .map(|(i, c)| (i, c.clone()))
            .collect();
        // The PARSER keeps the first `^ ` line and reads the second as a
        // paragraph, so a table that arrives with two captions loses one either
        // way. Reported rather than dropped in silence, and by the same rule the
        // parser uses, so the import and a re-read of its own output agree on
        // which one survives.
        for (i, _) in captions.iter().skip(1) {
            self.diag(
                HtmlImportDiagnosticCode::TableDegraded,
                "Dropped a second <caption>: a table has one caption, and the first one wins"
                    .into(),
                HtmlImportSeverity::Warning,
                &format!("{path}/caption[{}]", i + 1),
            );
        }
        let caption_children: Option<Vec<Handle>> = captions
            .first()
            .map(|(_, c)| c.children.borrow().iter().cloned().collect());
        let mut trs = Vec::new();
        let mut section_tags: Vec<String> = Vec::new();
        let mut section_nodes: Vec<(Handle, String)> = Vec::new();
        rows(
            h,
            path,
            None,
            &mut section_tags,
            &mut section_nodes,
            &mut trs,
        );
        // The attributes of the SECTIONS, read once and in document order,
        // parallel to `section_tags` so a row's section id indexes both. Only a
        // `<tbody>` has a slot for them - the body group `row_groups` states -
        // so `row_groups` TAKES what it places out of this and whatever is left
        // is reported below. Nothing read them before: a `<tbody id="totals">`
        // fell into the empty `attrs` slot with no diagnostic at all
        // (carve#1210).
        let mut sections = Sections {
            tags: section_tags,
            attrs: Vec::with_capacity(section_nodes.len()),
        };
        for (node, section_path) in &section_nodes {
            let own = self
                .attrs(node, section_path)
                .map(|a| (a, section_path.clone()));
            sections.attrs.push(own);
        }
        // A `<colgroup>` and its `<col>` children state COLUMN structure, and
        // Carve has no column model to put it in - not a narrower slot, none at
        // all. Dropped as it always was, said out loud now, because a loss the
        // reader is never told about is the one they cannot work around.
        for p in Self::column_groups(h, path) {
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                "Dropped <colgroup>: Carve has no column model, and a table's columns are only the cells its rows carry".into(),
                HtmlImportSeverity::Warning,
                &p,
            );
        }
        let source_cells = |tr: &Handle| -> Vec<Handle> {
            tr.children
                .borrow()
                .iter()
                .filter(|n| matches!(Self::tag(n).as_deref(), Some("td" | "th")))
                .cloned()
                .collect()
        };
        // PART 10 SST9 gives every `th` a `scope` from its POSITION: `col` in
        // the leading run of all-header rows, `row` for a header cell below it.
        // Read off the SOURCE rows, because the run ENDS at the first row
        // carrying a `td` and the built rows carry continuation cells that are
        // neither.
        let leading_header_rows = trs
            .iter()
            .take_while(|(tr, _)| {
                let cells = source_cells(tr);
                !cells.is_empty() && cells.iter().all(|n| Self::tag(n).as_deref() == Some("th"))
            })
            .count();
        // How many rows are left in each row's own group, INCLUDING it. Computed
        // once: asking per cell means scanning the whole table for each one,
        // which is quadratic in the row count.
        let mut totals: BTreeMap<Option<usize>, usize> = BTreeMap::new();
        for (_, section) in &trs {
            *totals.entry(*section).or_insert(0) += 1;
        }
        let mut seen: BTreeMap<Option<usize>, usize> = BTreeMap::new();
        let remaining_in_group: Vec<usize> = trs
            .iter()
            .map(|(_, section)| {
                let index = seen.entry(*section).or_insert(0);
                let left = totals.get(section).copied().unwrap_or(1) - *index;
                *index += 1;
                left
            })
            .collect();
        let mut built: Vec<Vec<BuiltCell>> = Vec::with_capacity(trs.len());
        for (r, (tr, _)) in trs.iter().enumerate() {
            let mut row = Vec::new();
            for (c, cell) in source_cells(tr).iter().enumerate() {
                let p = format!(
                    "{path}/tr[{}]/{}[{}]",
                    r + 1,
                    Self::tag(cell).unwrap(),
                    c + 1
                );
                let colspan = Self::span_count(cell, "colspan", MAX_COLSPAN, 1);
                // A rowspan stops at its ROW GROUP in HTML, and `rowspan="0"`
                // means exactly "to the end of it". Both are resolved against the
                // group the row is actually in, so a `<tfoot>` below the body is
                // not swallowed by a cell the layout stops at the body's last
                // row.
                let declared = Self::span_count(cell, "rowspan", MAX_ROWSPAN, 0);
                let left = remaining_in_group[r];
                let mut rowspan = if declared == 0 {
                    left
                } else {
                    declared.min(left)
                };
                // And it stops at the head the RENDERER will synthesize. Carve
                // derives the head from the leading run of all-header rows, so a
                // span reaching out of that run lands in a `<thead>` with its
                // other rows in the `<tbody>` - which browsers clip, making the
                // written table say something the source table did not. Clipped
                // here instead, where it can be reported: the alternative is a
                // document that claims a grid it does not render.
                if r < leading_header_rows && r + rowspan > leading_header_rows {
                    self.diag(
                        HtmlImportDiagnosticCode::TableDegraded,
                        "Clipped a rowspan at the header rows: Carve derives the head from the leading header rows, and a span leaving them crosses a boundary browsers clip anyway".into(),
                        HtmlImportSeverity::Warning,
                        &p,
                    );
                    rowspan = leading_header_rows - r;
                }
                row.push(BuiltCell {
                    cell: TableCell {
                        header: Self::tag(cell).as_deref() == Some("th"),
                        span: None,
                        align: None,
                        attrs: self.attrs(cell, &p),
                        children: self.inlines(&cell.children.borrow(), &p, depth + 1)?,
                        pos: None,
                    },
                    colspan,
                    rowspan,
                });
            }
            built.push(row);
        }
        // A `<tr>`'s own attributes have a slot - `TableRow::attrs`, which the
        // writer spells on the closing pipe and every renderer emits on the
        // `<tr>` - and went in silence before this. Reading them at all also
        // puts a `<tr>` on the ordinary attribute path, so an unsupported one
        // reports the way it does on any other element.
        let row_attrs: Vec<Option<Attrs>> = (0..trs.len())
            .map(|r| {
                let p = format!("{path}/tr[{}]", r + 1);
                self.attrs(&trs[r].0, &p)
            })
            .collect();
        let mut result = self.span_grid(built, &row_attrs, path, depth)?;

        // A `scope` equal to the positional default carries no information the
        // renderer cannot reproduce, and importing it would write this engine's
        // own output back as if the author had written it. A value the default
        // cannot explain - `colgroup`, `rowgroup`, which have no marker
        // spelling - is the only way to get it, so it stays (carve-rs#944).
        for (r, row) in result.iter_mut().enumerate() {
            for cell in row.cells.iter_mut() {
                if !cell.header {
                    continue;
                }
                let default = if r < leading_header_rows {
                    "col"
                } else {
                    "row"
                };
                let Some(attrs) = cell.attrs.as_mut() else {
                    continue;
                };
                if attrs.key_values.get("scope").map(String::as_str) == Some(default) {
                    attrs.key_values.remove("scope");
                }
            }
        }
        let header_rows: Vec<bool> = trs
            .iter()
            .map(|(tr, _)| {
                let cells = source_cells(tr);
                !cells.is_empty() && cells.iter().all(|n| Self::tag(n).as_deref() == Some("th"))
            })
            .collect();
        let row_groups = self.row_groups(
            &trs,
            &header_rows,
            &result,
            leading_header_rows,
            path,
            &mut sections,
        );
        // Whatever `row_groups` did not place. A `<thead>` and a `<tfoot>` have
        // no slot at all - the field states the head and the foot as row COUNTS
        // - and a `<tbody>`'s attributes reach nothing when the field itself was
        // not kept.
        let sections_with_rows: BTreeSet<usize> = trs.iter().filter_map(|(_, s)| *s).collect();
        for (id, slot) in sections.attrs.iter().enumerate() {
            let Some((own, own_path)) = slot else {
                continue;
            };
            let tag = sections.tags.get(id).map(String::as_str).unwrap_or("tbody");
            // A body group IS the run of rows it consumes, so a section with
            // none is not a group and has nowhere to put them. Stating it as a
            // zero-count group would put a body in the partition that describes
            // no rows.
            let reason = match tag {
                "thead" => "a table's head is stated as a row count and has no attribute slot",
                "tfoot" => "a table's foot is stated as a row count and has no attribute slot",
                _ if sections_with_rows.contains(&id) => {
                    "the row grouping this body belongs to was not kept, and nothing else holds it"
                }
                _ => "a body group is the rows it consumes, and this one has none",
            };
            self.diag(
                HtmlImportDiagnosticCode::AttributeDropped,
                format!(
                    "Dropped {} on <{tag}>: {reason}",
                    Self::attr_names(own).join(", ")
                ),
                HtmlImportSeverity::Warning,
                own_path,
            );
        }
        let caption = match caption_children {
            Some(kids) => Some(self.inlines(&kids, &format!("{path}/caption[1]"), depth + 1)?),
            None => None,
        };
        Ok(Table {
            attrs,
            caption,
            short_caption: None,
            rows: result,
            row_groups,
            pos: None,
        })
    }

    /// `<thead>` / `<tbody>` / `<tfoot>` to `Table::row_groups`, when the
    /// partition says something a reader cannot derive (carve#1210 D1, ruled as
    /// (b); pandoc-carve#61 implements the same rule and carve-js followed).
    ///
    /// Every renderer already derives a structure from the rows alone: the
    /// leading run of all-header rows is the head, everything after it is one
    /// body, there is no foot and there are no row-head columns. A `<thead>`
    /// over a `<tbody>` is exactly that, so emitting the field for it would put
    /// structure into every imported table that Carve source cannot spell and
    /// hand-written Carve never carries.
    ///
    /// So it is emitted only where the two DISAGREE: a `<tfoot>`, a second
    /// `<tbody>`, a body with its own intermediate header rows, a body with
    /// row-head columns, or a `<thead>` whose rows are not all header cells
    /// (Word and pandoc both emit `<thead><tr><td>`), where the derived head is
    /// empty and the stated one is not.
    ///
    /// A `<tbody>`'s own attributes are one of those disagreements: the derived
    /// structure has no way to say them, so a body carrying any is not derivable
    /// and the field is emitted to hold them in the body group's `attrs`. Only a
    /// BODY has that slot - the head and the foot are stated as counts - so a
    /// `<thead>` or `<tfoot>` that carries attributes is reported by the caller,
    /// along with a `<tbody>` whose group was dropped for another reason.
    ///
    /// The counts are NOT checked against `rows.len()` here. They are built from
    /// the same row list the rows are built from, so a check at this point
    /// cannot fail; PART 12 §15's MUST is enforced where a payload arrives from
    /// elsewhere, in `from_json`.
    fn row_groups(
        &mut self,
        trs: &[(Handle, Option<usize>)],
        header_rows: &[bool],
        rows: &[TableRow],
        leading_header_rows: usize,
        path: &str,
        sections: &mut Sections,
    ) -> Option<TableRowGroups> {
        if trs.is_empty() {
            return None;
        }
        let section_of = |index: usize| -> &str {
            trs[index]
                .1
                .and_then(|id| sections.tags.get(id))
                .map(String::as_str)
                .unwrap_or("tbody")
        };
        // The head is a PREFIX of `rows` and the foot a SUFFIX, which is what
        // the field can express. A `<thead>` that is not first, or a `<tfoot>`
        // with rows after it, is a table this cannot describe.
        let head_rows = (0..trs.len())
            .find(|&i| section_of(i) != "thead")
            .unwrap_or(trs.len());
        let mut foot_rows = 0;
        while foot_rows < trs.len() - head_rows && section_of(trs.len() - 1 - foot_rows) == "tfoot"
        {
            foot_rows += 1;
        }
        let middle = head_rows..trs.len() - foot_rows;
        if middle
            .clone()
            .any(|i| matches!(section_of(i), "thead" | "tfoot"))
        {
            self.diag(
                HtmlImportDiagnosticCode::TableDegraded,
                "Dropped the row grouping of a table whose <thead> or <tfoot> is not at the edge of its rows: the head is a prefix of the rows and the foot a suffix".into(),
                HtmlImportSeverity::Warning,
                path,
            );
            return None;
        }

        let mut bodies: Vec<TableBodyGroup> = Vec::new();
        let mut index = middle.start;
        while index < middle.end {
            let section = trs[index].1;
            let start = index;
            while index < middle.end && trs[index].1 == section {
                index += 1;
            }
            let mut group_head = 0;
            while start + group_head < index && header_rows[start + group_head] {
                group_head += 1;
            }
            // A group whose rows are ALL header rows is an intermediate header
            // with nothing under it, which is what the counts say and not
            // something to reinterpret.
            let body_start = start + group_head;
            let row_head_columns = if body_start < index {
                Self::row_head_columns(rows, body_start..index)
            } else {
                0
            };
            // The `<tbody>`'s own attributes: the body group is where the
            // exchanged model puts them, and a body is the only section with a
            // slot. TAKEN here rather than after the `derivable` return below,
            // because a body carrying any is never derivable - that is what the
            // clause says - so the field is returned whenever one was taken, and
            // the only return that skips this point is the one before the loop.
            let own = section
                .and_then(|id| sections.attrs.get_mut(id))
                .and_then(Option::take)
                .map(|(a, _)| a);
            bodies.push(TableBodyGroup {
                head_rows: group_head,
                body_rows: index - body_start,
                row_head_columns: (row_head_columns > 0).then_some(row_head_columns),
                attrs: own,
            });
        }

        // No `<thead>` at all: the leading run of header rows is what every
        // renderer reads as the head, so it is counted as one here too. Without
        // this, the ORDINARY table - a header row and some data rows, with only
        // the implicit `<tbody>` the HTML parser inserts - comes out with an
        // intermediate header and no head, which is a different statement about
        // the same table and puts the field on nearly every document.
        //
        // ONE body only. With a second one, the header-only first body is a
        // BOUNDARY the field exists to record, and absorbing it away leaves a
        // single ordinary body that the derivation reproduces.
        let mut head_rows = head_rows;
        if head_rows == 0 && bodies.len() == 1 && leading_header_rows > 0 {
            let absorbed = leading_header_rows.min(bodies[0].head_rows);
            head_rows = absorbed;
            bodies[0].head_rows -= absorbed;
            // A group carrying attributes is not empty, whatever its counts say:
            // dropping it here would drop them with it.
            if bodies[0].head_rows == 0
                && bodies[0].body_rows == 0
                && bodies[0].row_head_columns.is_none()
                && bodies[0].attrs.is_none()
            {
                bodies.clear();
            }
        }

        let derivable = head_rows == leading_header_rows
            && foot_rows == 0
            && bodies.len() <= 1
            && bodies.iter().all(|b| {
                b.head_rows == 0 && b.row_head_columns.unwrap_or(0) == 0 && b.attrs.is_none()
            });
        if derivable {
            return None;
        }
        // Carve SOURCE has no spelling for the field, so a writer loses it. The
        // AST keeps it and `html_to_carve` reports it, which is the split PART
        // 12 §16 draws.
        self.unspellable.push((
            path.to_owned(),
            "A table with an explicit head/body/foot grouping has no Carve spelling; the written table keeps only the structure a reader derives from its rows".into(),
        ));
        Some(TableRowGroups {
            head_rows,
            bodies,
            foot_rows,
        })
    }

    /// Leading COLUMNS that are header cells in every row of the group.
    ///
    /// Counted over the expanded grid rather than over the source cells, because
    /// columns and cells are not the same thing: `<th colspan="2">` is one
    /// element and two columns, and a `<th rowspan="2">` leaves the row below it
    /// starting with a data ELEMENT while a header still occupies the column.
    ///
    /// A `<` needs no resolution: `span_grid` builds a colspan continuation
    /// carrying its ORIGIN's header flag. A `^` is built with the flag cleared,
    /// because the cell it continues is in another row, so that one is resolved
    /// upward.
    ///
    /// One SLOT is one column here, which is what makes the count a simple walk.
    /// It is true because `span_grid` carries a mark into each column a spanning
    /// cell covers rather than one for its origin, so no `^` ever stands for
    /// more than one column. carve-js carries the single mark and needs the
    /// width of the origin to undo it; a width lookup here could not change an
    /// answer, so there is none.
    fn row_head_columns(rows: &[TableRow], group: std::ops::Range<usize>) -> usize {
        fn origin_row(rows: &[TableRow], r: usize, c: usize) -> Option<usize> {
            let mut up = r;
            while up > 0 {
                up -= 1;
                if rows[up].cells.get(c).and_then(|cell| cell.span).is_none() {
                    return Some(up);
                }
            }
            None
        }
        fn header_at(rows: &[TableRow], r: usize, c: usize) -> bool {
            let Some(cell) = rows[r].cells.get(c) else {
                return false;
            };
            if cell.span == Some(TableCellSpan::Rowspan) {
                return match origin_row(rows, r, c) {
                    Some(up) => header_at(rows, up, c),
                    None => false,
                };
            }
            cell.header
        }
        let leading = |r: usize| -> usize {
            let cells = &rows[r].cells;
            let mut columns = 0;
            while columns < cells.len() && header_at(rows, r, columns) {
                columns += 1;
            }
            // An all-header row would say every column is a row head, which is
            // what an intermediate HEADER row is, not a row-head column.
            if columns == cells.len() {
                0
            } else {
                columns
            }
        };
        group.map(leading).min().unwrap_or(0)
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
        if tag == "q" {
            // PART 10 has no quotation node and does not need one: `<q>` is
            // punctuation the author would otherwise have typed, and the marks
            // ARE the content once they are in the text. This is a deliberate
            // mapping, not an unwrap, so it reports nothing - the reverse of
            // the `element-unwrapped` info it used to emit, which claimed a
            // loss where the only thing lost is a tag whose entire rendered
            // effect has been written out.
            //
            // The pair alternates with nesting, the way every user agent
            // renders it: HTML5 leaves the marks to the UA and a nested
            // quotation that repeated the outer pair would be unreadable.
            //
            // The pair written is the English one, and the cases where a UA
            // would render a different pair are the ones carrying a `lang` -
            // which has no slot here and is already reported as a dropped
            // attribute by the walk below. So the locale is not silently
            // decided: the signal that would have chosen another pair is
            // itself the diagnostic.
            let attrs = self.attrs(h, path);
            let (open, close) = if self.quote_depth % 2 == 0 {
                ('\u{201c}', '\u{201d}')
            } else {
                ('\u{2018}', '\u{2019}')
            };
            self.quote_depth += 1;
            let inner = self.inlines(&h.children.borrow(), path, depth + 1);
            self.quote_depth -= 1;
            let mut quoted = vec![InlineNode::text(open.to_string())];
            quoted.extend(inner?);
            quoted.push(InlineNode::text(close.to_string()));
            // An id or a class on the element still has a home, and a span is
            // the one that keeps it without inventing a node for the quotation
            // itself.
            if attrs.is_some() {
                return Ok(vec![InlineNode::Span(Span {
                    attrs,
                    children: quoted,
                    injected: false,
                    pos: None,
                })]);
            }
            return Ok(quoted);
        }
        if tag == "math" {
            // MathML -> `math`, as carve#1210's D6 rules it: a three-tier
            // lookup for TeX the producer already put in the source, and no
            // MathML-to-TeX converter in any engine.
            //
            // THE DECISION IS THE THIRD TIER, and it is a drop rather than a
            // degrade. MathML's children are a token stream, so concatenating
            // them is not a lossy rendering of the equation but a different
            // value: the children of `<math><mfrac><mn>1</mn><mn>2</mn></mfrac>
            // </math>` concatenate to `12`, one half arriving as twelve. A
            // plausible wrong value survives review, where a missing equation
            // and a warning naming it do not. That is the line between math and
            // the embeds at the end of this method: a `<video>`'s children ARE
            // fallback content the author wrote for exactly this case.
            //
            // `roundtrip` keeps the whole element instead, by falling through
            // to the raw arm below. Carve's own HTML spells math as a `<span>`,
            // so a `<math>` reaching that mode is foreign markup by definition
            // and its contract is to preserve it verbatim.
            if let Some((tier, content)) = Self::math_tex(h) {
                // The subtree is charged here because the mapping returns
                // without walking it. `max_nodes` and `max_depth` must not
                // depend on which branch an element takes.
                self.charge_subtree(h, depth)?;
                let attrs = self.attrs(h, path);
                // Reported on the tier that SUPPLIED the content, not on which
                // one was available: an annotation holding only whitespace
                // falls through to `alttext`, and reading the presence of the
                // element would make that fall-through the one tier-2 read
                // that says nothing.
                if tier == 2 {
                    self.diag(
                        HtmlImportDiagnosticCode::ElementUnwrapped,
                        "Read <math> through its alttext: MathML does not declare the encoding of alttext, so TeX is assumed".into(),
                        HtmlImportSeverity::Info,
                        path,
                    );
                }
                return Ok(vec![InlineNode::Math(Math {
                    attrs,
                    display: Self::attr(h, "display").as_deref() == Some("block"),
                    content,
                    pos: None,
                })]);
            }
            if self.opts.mode != HtmlImportMode::Roundtrip {
                self.charge_subtree(h, depth)?;
                // No attribute walk on the way out: the element and everything
                // riding on it is gone, and this warning covers all of it.
                self.diag(
                    HtmlImportDiagnosticCode::ElementDropped,
                    "Dropped <math>: no TeX annotation and no alttext, and its children are a token stream, not an equation".into(),
                    HtmlImportSeverity::Warning,
                    path,
                );
                return Ok(Vec::new());
            }
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
            // `<ins>` has a marker of its own, `{+ +}`, which renders back to
            // `<ins>`. Without this branch it fell through to the unwrapping
            // path, so an insertion lost its element AND was reported as
            // unsupported markup - twice wrong, since Carve can spell it.
            "ins" => InlineNode::CriticInsert(CriticInsert {
                attrs,
                children,
                pos: None,
            }),
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
    import(html, options, false)
}

/// The import both entry points share. `writing` is what separates them: the
/// losses a WRITER takes are reported only when one runs (PART 12 §16).
fn import(
    html: &str,
    options: &HtmlImportOptions,
    writing: bool,
) -> Result<HtmlImportResult<Document>, HtmlImportError> {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let mut importer = Importer {
        opts: options,
        diagnostics: Vec::new(),
        nodes: 0,
        quote_depth: 0,
        unspellable: Vec::new(),
    };
    let children = importer.blocks(&dom.document.children.borrow(), "", 0)?;
    if writing {
        for (path, message) in std::mem::take(&mut importer.unspellable) {
            importer.diag(
                HtmlImportDiagnosticCode::StructureUnspellable,
                message,
                HtmlImportSeverity::Warning,
                &path,
            );
        }
    }
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
    let result = import(html, options, true)?;
    let value =
        render_carve(&result.value).map_err(|_: RenderDepthError| HtmlImportError::RenderDepth)?;
    Ok(HtmlImportResult {
        value,
        report: result.report,
    })
}
