//! HTML5-to-Carve migration boundary.

use crate::ast::*;
use crate::escape::is_dangerous_attr_name;
use crate::extension::{
    label_default, LABEL_CODE_GROUP, LABEL_ENDNOTES, LABEL_INDEX_BACKREF, LABEL_TABS_GROUP,
};
use crate::profile::ADMONITION_TIER1_KINDS;
use crate::render::{semantic_value_target, EXTENDED_SEMANTIC_SPAN_ORDER};
use crate::render_carve::is_attr_identifier;
use crate::{render_carve, RenderCarveError};
use html5ever::tendril::TendrilSink;
use html5ever::{namespace_url, ns, QualName};
use html5ever::{serialize, serialize::SerializeOpts};
use markup5ever_rcdom::{Handle, Node, NodeData, RcDom, SerializableHandle};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

/// Declares one of the import report's closed vocabularies together with the
/// spelling each variant takes on the wire.
///
/// The enum, [`ALL`](HtmlImportDiagnosticCode::ALL) and
/// [`as_str`](HtmlImportDiagnosticCode::as_str) come out of ONE table on
/// purpose. `resources/html-import-schema.json` in the spec fixes these
/// vocabularies, and the test that holds the crate to it can only look at the
/// variants the list hands it: a hand-written list would let a new variant
/// ship unlisted, unspelled by the schema, and unnoticed - the same
/// check-that-cannot-fail the test exists to remove, moved one level up.
/// Written this way the compiler refuses a variant that skips either.
macro_rules! report_vocabulary {
    (
        $(#[$enum_meta:meta])*
        $name:ident {
            $( $(#[$variant_meta:meta])* $variant:ident => $spelling:literal ),+ $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $( $(#[$variant_meta])* $variant, )+
        }

        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [Self] = &[ $( Self::$variant, )+ ];

            /// The variant's spelling in the JSON import report. The spec's
            /// `resources/html-import-schema.json` admits exactly the set
            /// these return.
            pub fn as_str(self) -> &'static str {
                match self {
                    $( Self::$variant => $spelling, )+
                }
            }

            /// The variant a wire spelling names, or `None` when the
            /// vocabulary has no such member. The inverse of `as_str`, so a
            /// caller that accepts a name accepts exactly what the report can
            /// write back.
            pub fn from_name(name: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|v| v.as_str() == name)
            }
        }
    };
}

report_vocabulary!(HtmlImportMode {
    Safe => "safe",
    Semantic => "semantic",
    Roundtrip => "roundtrip",
});

report_vocabulary!(HtmlImportAdapter {
    Generic => "generic",
    Tiptap => "tiptap",
    Prosemirror => "prosemirror",
    Ckeditor => "ckeditor",
    Tinymce => "tinymce",
    Word => "word",
    GoogleDocs => "google-docs",
});

report_vocabulary!(HtmlImportSeverity {
    Info => "info",
    Warning => "warning",
    Error => "error",
});

report_vocabulary!(HtmlImportDiagnosticCode {
    ElementDropped => "element-dropped",
    ElementUnwrapped => "element-unwrapped",
    AttributeDropped => "attribute-dropped",
    StyleUnmapped => "style-unmapped",
    TableDegraded => "table-degraded",
    RawPreserved => "raw-preserved",
    /// A structure the AST holds and Carve 0.1 SOURCE has no spelling for, so
    /// only a WRITER loses it (PART 12 §16). Reported by `html_to_carve`;
    /// `html_to_ast` keeps the structure and says nothing.
    StructureUnspellable => "structure-unspellable",
    /// One element became SEVERAL Carve blocks because writing it as one would
    /// have changed what it says (PART 12 §16). The ruled case is a `<dl>` whose
    /// empty `<dd>` is not last: dropping the unspellable description would let
    /// the entry below lend the term above its own, so the list breaks instead
    /// (markup-carve/carve#1638).
    ///
    /// NOT PRODUCED BY THIS ENGINE YET, and named here because the published
    /// vocabulary is the schema's and this enum is only its spelling -
    /// `every_report_vocabulary_is_exactly_the_one_the_schema_admits` compares
    /// the two as sets, in both directions. The behavior is
    /// markup-carve/carve-rs#1312, and it belongs in the WRITER rather than
    /// here: like `structure-unspellable`, an empty description survives into
    /// the AST intact and only a writer loses it. The gap is pinned by
    /// `BEHIND_THE_RULING` in `tests/html_import.rs`, which fails in both
    /// directions, so it cannot go quiet.
    StructureSplit => "structure-split",
    /// The source did not declare how to read a value and this importer picked
    /// an encoding anyway, so the node it produced is only correct if that
    /// guess holds.
    ///
    /// Deliberately NOT `ElementUnwrapped`: unwrapping is a note about the
    /// input's structure and loses no meaning, while an assumed encoding is a
    /// warning about the OUTPUT. A consumer told only that an element is gone
    /// cannot tell a harmless structural event from content that may be in the
    /// wrong language entirely, and that is the one signal it could act on.
    EncodingAssumed => "encoding-assumed",
    DiagnosticsTruncated => "diagnostics-truncated",
});

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
    /// The `labels` map the HTML was RENDERED with (PART 9 §16a).
    ///
    /// The derived-name drop matches the English defaults, which catches a
    /// document rendered in English and nothing else: one rendered with
    /// `tabsGroup = "Registerkarten"` carries a value no default equals, so its
    /// generated name is kept and baked into the imported source - and a
    /// translated document is exactly the one the map exists to serve
    /// (markup-carve/carve#1500 step 2).
    ///
    /// The host that rendered the HTML knows the map it used; passing the same
    /// one here closes that. Layered OVER the defaults, so naming one key
    /// leaves every other construct matched as before. Empty changes nothing.
    pub labels: BTreeMap<String, String>,
}

impl Default for HtmlImportOptions {
    fn default() -> Self {
        Self {
            mode: HtmlImportMode::Safe,
            adapter: HtmlImportAdapter::Generic,
            max_depth: 128,
            max_nodes: 1_000_000,
            max_diagnostics: 1_000,
            labels: BTreeMap::new(),
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
    SourceUnspellable,
}

struct Importer<'a> {
    opts: &'a HtmlImportOptions,
    /// Every diagnostic, paired with the document position of the LOSING
    /// ELEMENT. The vector is built in construction order and sorted by that
    /// position on the way out, so a tie keeps the order the rows were built
    /// in - which for one element's attributes is the order it spells them.
    diagnostics: Vec<(usize, HtmlImportDiagnostic)>,
    /// Every node of the parsed tree, numbered in DOCUMENT ORDER
    /// (markup-carve/carve#1586).
    ///
    /// A REPORT IS ORDERED BY WHERE THE LOSS IS, NOT BY WHEN IT WAS NOTICED.
    /// docs/html-import.md always said the diagnostic list is ordered and,
    /// until that ticket, never said ordered by what - so each engine's list
    /// came out in whatever order its own walk happened to construct the rows
    /// in. This importer reads a table's cells before its `<caption>`, because
    /// the caption fills a slot on the finished table, so a `<table>` losing
    /// something on both reported the cell first and the caption second for a
    /// document that spells them the other way round.
    ///
    /// Numbering the tree once and sorting at the end fixes the whole class
    /// rather than that one shape: no handler has to be rewritten to visit its
    /// parent's children in source order, and none can reintroduce the defect
    /// by choosing a convenient traversal.
    ///
    /// Keyed by node ADDRESS, and the entry holds the handle it was keyed by -
    /// the same rule `footnote_refs` follows below, and for the same reason.
    /// The footnote pass DETACHES nodes, so a key whose node had been dropped
    /// would be an address the allocator may hand to a live node next.
    document_order: HashMap<usize, (Handle, usize)>,
    nodes: usize,
    /// How many `<q>` elements are open around the node being read. HTML5
    /// leaves the marks to the user agent and every one of them alternates, so
    /// the depth is what chooses between the double and the single pair.
    quote_depth: usize,
    /// The losses a WRITER takes, held back until one writes (PART 12 §16).
    unspellable: Vec<(Handle, String, String)>,
    /// The reference sites the adapter footnote pass recognized: the node
    /// `inline` must read as a footnote reference, and the label it carries.
    ///
    /// Keyed by node ADDRESS, and the entry holds the handle it was keyed by -
    /// which is what makes the address a valid key. A key whose node had been
    /// dropped would be an address the allocator may hand to a live node next,
    /// so the map pins every node it can answer for.
    footnote_refs: HashMap<usize, (Handle, String)>,
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

/// The elements a footnote definition body can be spelled as.
const FOOTNOTE_DEFINITION_BLOCKS: [&str; 7] =
    ["li", "div", "section", "aside", "p", "td", "blockquote"];

/// The elements a per-footnote wrapper can be spelled as.
///
/// Word wraps each definition in `<div style='mso-element:footnote' id=ftn1>`
/// and LibreOffice in `<div id="sdfootnote1">`, so the block holding the body
/// is one level above the paragraph the back-anchor sits in.
const FOOTNOTE_WRAPPER_BLOCKS: [&str; 4] = ["div", "li", "section", "aside"];

/// A footnote-reference candidate: the anchor, the definition block its
/// fragment resolves to, and the fragment itself.
struct FootnoteCandidate {
    reference: Handle,
    block: Handle,
    fragment: String,
}

/// One recognized note: its block and every reference bound to it.
struct FootnoteGroup {
    block: Handle,
    refs: Vec<Handle>,
    fragments: Vec<String>,
}

impl<'a> Importer<'a> {
    /// The engine-written string for `key` as the RENDER used it: the host's
    /// map first, then the English default.
    fn label(&self, key: &str) -> String {
        self.opts
            .labels
            .get(key)
            .cloned()
            .unwrap_or_else(|| label_default(key).to_string())
    }

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
        node: &Handle,
    ) {
        if self.diagnostics.len() >= self.opts.max_diagnostics {
            if let Some(last) = self.diagnostics.last_mut() {
                // `usize::MAX`, so the marker stays where a reader needs it -
                // at the END of the report - rather than sorting to wherever
                // the row it replaced happened to sit.
                *last = (
                    usize::MAX,
                    HtmlImportDiagnostic {
                        code: HtmlImportDiagnosticCode::DiagnosticsTruncated,
                        message: "HTML import diagnostics limit reached".into(),
                        severity: HtmlImportSeverity::Error,
                        path: None,
                    },
                );
            }
            return;
        }
        let at = self.position_of(node);
        self.diagnostics.push((
            at,
            HtmlImportDiagnostic {
                code,
                message,
                severity,
                path: Some(path.into()),
            },
        ));
    }

    /// Where a losing element sits in the document, as the number the
    /// pre-order walk gave it.
    ///
    /// An element the HTML parser IMPLIED - a `<tbody>` around rows nobody
    /// wrote one for - is not in the source at all and has no position of its
    /// own, so it answers with its nearest ancestor's and ties with it. That is
    /// the honest reading: the loss is at the place in the source where the
    /// implied element's content begins.
    fn position_of(&self, node: &Handle) -> usize {
        let mut current = Some(node.clone());
        while let Some(handle) = current {
            if let Some((_, at)) = self.document_order.get(&node_key(&handle)) {
                return *at;
            }
            current = parent_handle(&handle);
        }
        usize::MAX
    }

    /// Number every node of the parsed tree in document order.
    ///
    /// Iterative, not recursive: the walk runs before `enter` has capped
    /// anything, so it meets whatever depth the input actually parsed to, and a
    /// recursive version would answer a deeply nested document with a stack
    /// overflow instead of the depth-limit error the page promises.
    fn number_document_order(&mut self, root: &Handle) {
        let mut stack = vec![root.clone()];
        let mut next = 0usize;
        while let Some(handle) = stack.pop() {
            self.document_order
                .insert(node_key(&handle), (handle.clone(), next));
            next += 1;
            for child in handle.children.borrow().iter().rev() {
                stack.push(child.clone());
            }
        }
    }

    /// The report, in the order docs/html-import.md states.
    fn report(&mut self) -> Vec<HtmlImportDiagnostic> {
        let mut entries = std::mem::take(&mut self.diagnostics);
        entries.sort_by_key(|(at, _)| *at);
        entries
            .into_iter()
            .map(|(_, diagnostic)| diagnostic)
            .collect()
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
                // A RENDERED CALLOUT IS AN `<aside>`, so the tag has to reach
                // `block` for `container_from` to rebuild it - flattened into an
                // inline run it was never offered (carve-rs#1240). It is a block
                // element in HTML and carve-js has always listed it here; the
                // omission also meant a bare `<aside>`'s own children were
                // unwrapped as inlines rather than kept as the blocks they are.
                | "aside"
                | "main"
                | "nav"
                | "header"
                | "footer"
                | "figure"
                // Synthetic, never present in real HTML input: the marker
                // `mark_footnote_placement` leaves where a non-final endnotes
                // section sat. It belongs here because it STANDS WHERE A
                // `<section>` STOOD, and a name this list does not recognize is
                // buffered as INLINE - which put the placement inside the
                // paragraph after it rather than between the two.
                | FOOTNOTE_PLACEMENT_TAG
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
    /// Carve's OWN HTML spelling of math, read back as a `math` node.
    ///
    /// `<span class="math inline">\(x\)</span>` and its `math display` twin are
    /// what this engine's HTML renderer writes for `` $`x` `` / `` $$`x` ``
    /// (PART 9 §18: `math_inline = '$', code_span`), and what djot.js and
    /// pandoc write as well - so this is a shape an importer meets rather than
    /// one engine's private convention.
    ///
    /// Without this the element fell through to the generic attributed-span
    /// writer and an equation came back as `[\\(x\\)]{.math .inline role=math}`,
    /// with no diagnostic. THAT LOSS IS INVISIBLE TO THE OBVIOUS CHECK:
    /// re-rendering the broken import produces byte-identical HTML, because a
    /// span carrying the same classes renders the same tag. What is gone is the
    /// NODE - and with it every non-HTML target, each of which has a math case
    /// it can no longer reach. Assert on a re-parse, never on bytes.
    ///
    /// TWO INDEPENDENT SIGNALS HAVE TO AGREE - the `math inline` / `math
    /// display` class PAIR, and a payload delimited to match it. Either alone
    /// is not evidence: a stylesheet is free to name a class `math`, and
    /// `\(x\)` in running prose is a pair of escaped parens - §18 spells the
    /// INPUT form as a `$` prefix on a code span and has no `\(…\)` form at
    /// all, so the delimiters are an output convention, not syntax. Requiring
    /// both also lets the class decide which delimiter to expect, so a
    /// `math display` span holding `\(…\)` is left alone rather than quietly
    /// re-labeled as display math.
    ///
    /// The payload is read off the DIRECT children ([`Self::direct_text`]),
    /// never through the recursive [`Self::text`]. An element child ends the
    /// read, which is also the right answer - a delimiter payload is text - and
    /// it keeps the recognition free of the recursion that [`Self::flat_text`]
    /// exists to avoid on the `<math>` arm.
    ///
    /// The BLOCK form, `<div class="math display">`, comes through here too,
    /// from the `div` arm of [`Self::block`], and it takes the CORE display
    /// spelling rather than the ```` ```math ```` extension fence
    /// (PART 9 §18, ruled at markup-carve/carve#1514). The class decides the
    /// mode either way, so a div spelled `math inline` writes the inline form
    /// rather than being promoted by the position it was found in.
    fn carve_math(h: &Handle, attrs: Option<&Attrs>) -> Option<Math> {
        let attrs = attrs?;
        if !attrs.classes.iter().any(|c| c == "math") {
            return None;
        }
        let display = attrs.classes.iter().any(|c| c == "display");
        // Not one element in both states: `math inline display` names no shape
        // the renderer can write, so it is a span with three classes rather
        // than an equation. Neither class present is the same non-answer.
        if display == attrs.classes.iter().any(|c| c == "inline") {
            return None;
        }
        let (open, close) = if display {
            ("\\[", "\\]")
        } else {
            ("\\(", "\\)")
        };
        let text = Self::direct_text(h)?;
        let payload = text
            .trim()
            .strip_prefix(open)?
            .strip_suffix(close)?
            .to_string();
        // Carve's math content is a `code_span`, one line by construction, so a
        // payload folded across source lines has exactly one spelling: the
        // whitespace run collapsed the way every other imported text run is.
        // TeX reads a newline as whitespace, so the equation is unchanged; a
        // `math` node holding a newline would not be writable at all.
        let content = collapse(&payload).trim().to_string();
        // `\(\)` carries the delimiters and no equation. There is no empty math.
        if content.is_empty() {
            return None;
        }
        Some(Math {
            attrs: Self::math_attrs(attrs, display),
            display,
            content,
            pos: None,
        })
    }

    /// The concatenated text of a node's DIRECT children, or `None` as soon as
    /// one of them is not a text node. Bounded and non-recursive, unlike
    /// [`Self::text`].
    fn direct_text(h: &Handle) -> Option<String> {
        let mut out = String::new();
        for child in h.children.borrow().iter() {
            let NodeData::Text { contents } = &child.data else {
                return None;
            };
            out.push_str(&contents.borrow());
        }
        Some(out)
    }

    /// What is left of a math span's attributes once recognition has eaten its
    /// own - the same bargain `attrs` already strikes for `<math>`'s `xmlns`:
    /// an attribute the branch READS and the renderer writes back from the node
    /// is consumed, not kept, or the document would spell it twice.
    ///
    /// The FIRST of each class only: `class="math math"` keeps the second as an
    /// author class, because the renderer writes the base pair once. Everything
    /// else the author put on the element - `id`, further classes, `data-*` -
    /// rides along and survives the round trip. A `role` saying anything other
    /// than `math` is the author's and stays.
    fn math_attrs(attrs: &Attrs, display: bool) -> Option<Attrs> {
        let mut out = attrs.clone();
        if let Some(i) = out.classes.iter().position(|c| c == "math") {
            out.classes.remove(i);
        }
        let base = if display { "display" } else { "inline" };
        if let Some(i) = out.classes.iter().position(|c| c == base) {
            out.classes.remove(i);
        }
        if out.key_values.get("role").map(String::as_str) == Some("math") {
            out.key_values.remove("role");
        }
        if out.id.is_none() && out.classes.is_empty() && out.key_values.is_empty() {
            return None;
        }
        Some(out)
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
                // THE REFUSAL IS DERIVED, NOT ENUMERATED. `is_dangerous_attr_name`
                // is the PART 9 §25 name filter the HTML renderer already
                // applies, so the importer refuses exactly what the renderer
                // would blank and the two cannot drift apart. Spelling a second
                // `starts_with("on")` here agreed on handlers and diverged on
                // `srcdoc` and `formaction`, which the renderer refuses and the
                // importer used to keep as "unsupported" by accident of the
                // keep list ending before them (carve-rs#1060).
                if is_dangerous_attr_name(&name) {
                    // The handler wording is a SHARED CONTRACT: the spec's
                    // `html-import` report fixtures pin it byte for byte, so
                    // the two sinks the filter adds are named separately rather
                    // than folded into one message that would move it.
                    let what = if name.to_ascii_lowercase().starts_with("on") {
                        "event-handler"
                    } else {
                        "active-content"
                    };
                    self.diag(
                        HtmlImportDiagnosticCode::AttributeDropped,
                        format!("Dropped {what} attribute {name} on <{tag}>"),
                        HtmlImportSeverity::Warning,
                        path,
                        handle,
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
                } else if name == "style" {
                    self.diag(
                        HtmlImportDiagnosticCode::StyleUnmapped,
                        "CSS declarations were not mapped".into(),
                        HtmlImportSeverity::Info,
                        path,
                        handle,
                    );
                } else if matches!(
                    (tag.as_str(), name.as_str()),
                    ("a", "href")
                        | ("img", "src" | "alt")
                        // A link's and an image's `title` is READ straight off
                        // the element into `Link.title` / `Image.title` and
                        // written back from the destination slot. The keep list
                        // spelled this as `title && tag != "a" && tag != "img"`;
                        // the refusal list has to spell it here instead, or the
                        // attribute comes out TWICE - once in the slot and once
                        // as `{title=…}` (carve-rs#1060).
                        | ("a" | "img", "title")
                        | ("ol", "start" | "type")
                        | ("td" | "th", "rowspan" | "colspan")
                        // READ by the math branch and carried to the node, so
                        // reporting them dropped would name a loss that does
                        // not happen. `xmlns` is the namespace declaration that
                        // makes the element MathML in the first place: consumed
                        // by having been recognized, not discarded.
                        | ("math", "display" | "alttext" | "xmlns")
                ) {
                    // CONSUMED by the branch that builds this node, and written
                    // back from there. Keeping it here as well would spell the
                    // same string twice, and diagnosing it would name a loss
                    // that does not happen.
                } else if name == "data-djot-src" || name == "data-carve-src" {
                    // The round-trip provenance markers this engine WRITES.
                    // Reading one back as an ordinary attribute would let an
                    // import restate a source the document no longer has.
                    self.diag(
                        HtmlImportDiagnosticCode::AttributeDropped,
                        format!("Dropped round-trip marker attribute {name} on <{tag}>"),
                        HtmlImportSeverity::Info,
                        path,
                        handle,
                    );
                } else if is_semantic_span_tag(&tag) && name == tag {
                    // THE MARKER OWNS THIS KEY. A compact semantic span is
                    // written `[t]{cite}`, so the tag name becomes an attribute
                    // key on the way out. An element carrying an attribute of
                    // its own name would have it overwritten by that marker,
                    // silently: `<cite cite="https://x">` stored the key twice
                    // and the URL lost to the empty marker value. The keep list
                    // hid this by refusing `cite` on everything but a
                    // `<blockquote>`; naming the drop is the honest form
                    // (carve-rs#1060).
                    self.diag(
                        HtmlImportDiagnosticCode::AttributeDropped,
                        format!("Dropped attribute {name} on <{tag}>: the semantic span's marker owns that key"),
                        HtmlImportSeverity::Info,
                        path,
                        handle,
                    );
                } else if name == "srcset" {
                    // REFUSED ON THE WAY IN, and the one refusal here that is
                    // not derived. This is the IMPORTER declining to admit a
                    // list-valued URL attribute the keep list never reached,
                    // which is a separate decision from what the renderer does
                    // with one an author wrote by hand:
                    //
                    //     srcset="safe.png 1x, javascript:alert(1) 2x"
                    //
                    // The §25 half is markup-carve/carve#1320, now ruled and
                    // implemented - `sanitize_attr_value` probes a URL-list
                    // value at every candidate rather than at its head. The
                    // refusal here neither waits on that nor duplicates it:
                    // admitting the attribute would be widening retention, and
                    // that is its own call.
                    self.diag(
                        HtmlImportDiagnosticCode::AttributeDropped,
                        format!("Dropped list-valued URL attribute {name} on <{tag}>"),
                        HtmlImportSeverity::Warning,
                        path,
                        handle,
                    );
                } else if !is_attr_identifier(&name) {
                    // No BARE spelling in Carve attribute syntax. The writer's
                    // `escape_attr_key` strips every character the rule
                    // rejects, so keeping `xlink:href` would emit `xlinkhref`
                    // and the document would claim an attribute nobody wrote.
                    // Losing it loudly beats renaming it quietly.
                    self.diag(
                        HtmlImportDiagnosticCode::AttributeDropped,
                        format!("Dropped attribute {name} on <{tag}>: not a Carve attribute name"),
                        HtmlImportSeverity::Info,
                        path,
                        handle,
                    );
                } else {
                    // EVERYTHING ELSE IS KEPT. Carve's attribute syntax can
                    // hold the pair, the renderer refuses what is dangerous,
                    // and the author wrote it. Dropping `aria-label` was an
                    // accessibility regression applied silently and in bulk to
                    // exactly the documents an importer runs on (carve-rs#1060).
                    out.key_values.insert(name, value);
                }
            }
        }
        self.drop_derived(handle, &mut out);
        if out == Attrs::default() {
            None
        } else {
            Some(out)
        }
    }

    /// Whether unwrapping this element removes anything an author wrote.
    ///
    /// The wrapper half of PART 9 §16a's reconstructability test: a `<section
    /// role="doc-endnotes">` is one the renderer writes and no Carve construct
    /// spells, so its removal is not a structural loss to report. It is the
    /// ELEMENT question; `derived_attributes` answers the ATTRIBUTE one, and a
    /// wrapper can be derived while an attribute on it is the author's - which
    /// is why the two are asked separately at the unwrap site.
    fn is_derived_wrapper(&self, handle: &Handle, tag: &str) -> bool {
        tag == "section" && Self::attr(handle, "role").as_deref() == Some("doc-endnotes")
    }

    /// Remove the attributes whose value EQUALS what the renderer derives for
    /// this element (PART 9 §16a, markup-carve/carve#1500, reconciled with
    /// Extensions §1.5 in markup-carve/carve#1511).
    ///
    /// THE RULE IS VALUE-MATCHED, NOT NAME-MATCHED. Nothing in the HTML says
    /// who wrote an attribute, so provenance cannot be the test and is not one.
    /// Where the value equals the generated one the output is identical
    /// whichever side wrote it, so the drop is a no-op for what a reader hears;
    /// where it differs the attribute is KEPT, always. That second half is what
    /// a blanket `aria-label` drop cost before (carve-rs#1060), and this rule
    /// does not spend it again.
    ///
    /// WHAT IT BUYS is the only thing keeping a `labels` map alive across an
    /// import. A kept `aria-label="Tabs"` is indistinguishable from an authored
    /// one, and PART 9 §12's author-wins precedence then makes the imported
    /// copy WIN on every later render: the same source re-rendered with
    /// `tabsGroup` set to `Registerkarten` still says `Tabs`. The document has
    /// been permanently unlocalized while no byte of today's output moved -
    /// which is also why a round trip cannot detect this and the test asserts
    /// ABSENCE instead.
    ///
    /// NOTHING IS DIAGNOSED. The renderer writes the value back, so this is not
    /// a lossy decision and emits no `attribute-dropped` - the same reason the
    /// `<figure>` and `<blockquote cite>` imports report nothing. A drop of the
    /// OTHER kind, where the value could not be represented, is diagnosed above
    /// as it always was.
    ///
    /// WHAT "WRITES IT BACK" MEANS HERE, measured rather than assumed. In this
    /// engine none of these constructs survives an import AS a construct yet -
    /// a fence comes back with no language, a tab set as bare divs
    /// (markup-carve/carve-php#1543) - so today's re-render names none of them
    /// again. Kept, the pair rides a plain `<pre><code>` and announces literal
    /// source as an image called `mermaid`, or gives a `<div class="tabs">` with
    /// no tabs in it a landmark named `Tabs`. The element is no longer the thing
    /// the name describes, so keeping it preserves a FALSE name rather than an
    /// accessible one. When the construct survives, the renderer names it again
    /// and the drop becomes the exact no-op §16a describes. Whether it does is
    /// also not decidable from here: it depends on which extensions the NEXT
    /// render registers, the same blind spot §16a already states and accepts.
    ///
    /// IT CATCHES THE DEFAULT ONLY, which §16a states as an accepted limit:
    /// HTML rendered with a German map carries a value no default equals, so it
    /// is kept and laundered. An importer cannot know the render's
    /// configuration and a non-default value is indistinguishable from an
    /// authored one, so failing SAFE - keep - is the side to err on.
    fn drop_derived(&self, handle: &Handle, out: &mut Attrs) {
        let Some(derived) = self.derived_attributes(handle, &out.classes) else {
            return;
        };
        for (name, values) in derived {
            // `id` HAS ITS OWN SLOT, and reading only `key_values` meant a
            // derived value there could never be found: the id of a titled
            // admonition's own title paragraph is the renderer's counter, and a
            // rule for it was a check that could not fire (carve-rs#1240).
            if name == "id" {
                if out.id.as_ref().is_some_and(|id| values.contains(id)) {
                    out.id = None;
                    out.order.retain(|slot| *slot != AttrSlot::Id);
                }
                continue;
            }
            // The importer stores an attribute under the name html5ever hands
            // it, and a Carve author may have written `ARIA-LABEL`; the
            // renderer's own author-wins check is ASCII-case-insensitive, so
            // the match here is too.
            let Some(key) = out
                .key_values
                .keys()
                .find(|held| held.eq_ignore_ascii_case(name))
                .cloned()
            else {
                continue;
            };
            if values.iter().any(|value| out.key_values[&key] == *value) {
                out.key_values.remove(&key);
            }
        }
    }

    /// What the renderer derives for this element, as an attribute name and the
    /// values it can produce there. A name absent here is one the renderer
    /// never writes for this element, so it is the author's and is kept
    /// untouched.
    ///
    /// The classes are the ones the renderers write at their DEFAULT options:
    /// an importer cannot see a host's `wrapper_class`, the same blind spot the
    /// default-only label match already accepts.
    ///
    /// A TITLE PARAGRAPH'S COUNTER ID IS HERE NOW. It was not, and the reason
    /// was that the check could not fire: `<aside>` was not a block tag, so a
    /// canonical admonition was unwrapped and its title paragraph flattened
    /// into the surrounding inline run, and the id was gone before any drop
    /// could reach it. carve-rs#1240 made the aside survive, so the family
    /// lands with it - as the note that stood here said it would.
    fn derived_attributes(
        &self,
        handle: &Handle,
        classes: &[String],
    ) -> Option<Vec<(&'static str, Vec<String>)>> {
        let tag = Self::tag(handle)?;
        let has = |name: &str| classes.iter().any(|class| class == name);

        // A DIAGRAM FENCE names itself after its own class word, which is why
        // Extensions §1.5 gives it no `labels` key - there is no fixed English
        // string to translate, so the derived value is readable off the
        // element. The role travels with the name and is derived whichever side
        // wrote the name, so it goes even where an authored name stays.
        //
        // `<pre>` ONLY, though the json-mode fences wrap in a `<div>`. That
        // mode puts the payload in a `<script>` the importer drops, so such a
        // div never comes back as a fence for a renderer to name again - the
        // drop would be a pure loss there, and a classed `<div role="img">` is
        // far likelier to be some other producer's than a `<pre>` is.
        if tag == "pre" && Self::attr(handle, "role").as_deref() == Some("img") {
            let word = classes.first()?;
            return Some(vec![
                ("role", vec!["img".to_string()]),
                ("aria-label", vec![word.clone()]),
            ]);
        }

        // AN ENDNOTES SECTION IS A DERIVED WRAPPER, and both attributes on it
        // are derived with it. PART 9 §16 writes the section around the notes
        // whenever the document has any, and §16a's test is
        // RECONSTRUCTABILITY: the role is fixed and the accessible name is the
        // documented default of the `endnotes` labels key, so both are
        // rebuildable from the element being read. A name no default matches is
        // the author's and is kept, exactly as for the tab set below.
        if self.is_derived_wrapper(handle, &tag) {
            return Some(vec![
                ("role", vec!["doc-endnotes".to_string()]),
                ("aria-label", vec![self.label(LABEL_ENDNOTES)]),
            ]);
        }

        // A TAB SET / CODE GROUP takes its name from a `labels` key, so unlike
        // the fence an author may genuinely have written the same words. Only
        // the documented English default is dropped; anything else is kept.
        // Both roles the tab set derives go - `group` in the `css` mode and
        // `tablist` in the `aria` one - because the element derives each.
        if tag == "div" && has("tabs") {
            return Some(vec![
                ("role", vec!["group".to_string(), "tablist".to_string()]),
                ("aria-label", vec![self.label(LABEL_TABS_GROUP)]),
            ]);
        }
        if tag == "div" && has("code-group") {
            return Some(vec![
                ("role", vec!["group".to_string()]),
                ("aria-label", vec![self.label(LABEL_CODE_GROUP)]),
            ]);
        }

        // A `css`-MODE PANEL is named by its own tab's `[label]` - a string the
        // author already wrote once, in the document, which is why §16a keeps
        // it out of the map. The importer reads that same string off the
        // control beside the panel rather than inventing it.
        if tag == "div" && (has("tabs-panel") || has("code-group-panel")) {
            let label_class = if has("tabs-panel") {
                "tabs-label"
            } else {
                "code-group-label"
            };
            let mut derived = vec![("role", vec!["group".to_string()])];
            if let Some(name) = Self::preceding_label_text(handle, label_class) {
                derived.push(("aria-label", vec![name]));
            }
            return Some(derived);
        }

        // AN INDEX BACK-LINK is named `{indexBackref} {term}`, or with the
        // occurrence ordinal appended for the kth of several. Both halves are
        // on the page - the term is the entry's own text, the ordinal is the
        // link's position among its siblings - so the whole value is
        // reconstructable and the match stays exact.
        if tag == "a" && has("index-backref") {
            return self
                .index_backref_names(handle)
                .map(|names| vec![("aria-label", names)]);
        }

        // A TITLED ADMONITION's title paragraph carries the renderer's own
        // document-order counter, and the `<aside>`'s `aria-labelledby` points
        // at it. Baked into source the id is authored, so the next render's
        // counter collides with it. The Nth such paragraph derives exactly
        // `adm-N`, so this stays an equality match rather than a guess at the
        // shape (carve-js#1296's family, reachable here since carve-rs#1240 made
        // the aside survive the import).
        if tag == "p" && has("admonition-title") && is_counted_admonition_title(handle) {
            return admonition_title_id(handle).map(|id| vec![("id", vec![id])]);
        }

        // THE REFERENCE GOES WITH THE ELEMENT IT NAMES. What makes this derived
        // is not where the id came from but that the paragraph is CONSUMED: it
        // becomes the container's title, so an `aria-labelledby` still pointing
        // at it names nothing. The renderer writes a fresh one on the next
        // render, so keeping this could only preserve a dangling name - the
        // defect markup-carve/carve-php#1542 records.
        if tag == "aside" && has("admonition") {
            let mut derived: Vec<(&'static str, Vec<String>)> = Vec::new();
            // AN UNTITLED CALLOUT IS NAMED BY ITS TYPE WORD, through the
            // `labels` key for that kind - the shape §16a's own example uses.
            // Unreachable until carve-rs#1240, because the `<aside>` was
            // unwrapped and there was no element left to read a name off; a
            // `::: note` therefore came back carrying `{aria-label=Note}` and
            // was permanently unlocalizable, the exact cost the clause prevents.
            if let Some(kind) = classes.iter().find(|c| {
                c.as_str() != "admonition" && ADMONITION_TIER1_KINDS.contains(&c.as_str())
            }) {
                let mut key = String::from("admonition");
                let mut chars = kind.chars();
                if let Some(first) = chars.next() {
                    key.extend(first.to_uppercase());
                    key.push_str(chars.as_str());
                }
                let default = label_default(&key);
                if !default.is_empty() {
                    derived.push(("aria-label", vec![default.to_string()]));
                }
            }
            // A TITLED one points at its title paragraph instead, and the
            // renderer writes one form or the other rather than both.
            let title = handle
                .children
                .borrow()
                .iter()
                .find(|child| is_counted_admonition_title(child))
                .cloned();
            if let Some(id) = title.and_then(|node| Self::attr(&node, "id")) {
                derived.push(("aria-labelledby", vec![id]));
            }
            if !derived.is_empty() {
                return Some(derived);
            }
        }

        None
    }

    /// The text of the tab control that names the panel `handle`: the nearest
    /// preceding ELEMENT sibling, when it is the one carrying `label_class`.
    /// Nearest-and-only, because a panel with no control before it - a fragment
    /// cut mid-set - derives no name, and guessing one there would drop a label
    /// nothing writes back.
    ///
    /// Read with [`Self::flat_text`], the explicit-stack walk, and not with the
    /// recursive `text`: this runs off `attrs`, before the depth counter has
    /// charged the subtree, and the importer's depth limit is a COUNTER a
    /// caller may raise past what the native stack holds.
    fn preceding_label_text(handle: &Handle, label_class: &str) -> Option<String> {
        let parent = parent_handle(handle)?;
        let siblings = parent.children.borrow();
        let at = siblings.iter().position(|node| Rc::ptr_eq(node, handle))?;
        for previous in siblings[..at].iter().rev() {
            if Self::tag(previous).is_none() {
                continue;
            }
            return has_class(previous, label_class).then(|| Self::flat_text(previous));
        }
        None
    }

    /// The names the index extension can derive for one back-link: the label
    /// plus the entry's term, and the same with this link's occurrence ordinal.
    /// Both spellings are accepted for a lone link because the extension's byte
    /// budget can truncate a numbered run down to one, leaving `… 1` on the
    /// survivor.
    fn index_backref_names(&self, handle: &Handle) -> Option<Vec<String>> {
        let parent = parent_handle(handle)?;
        let siblings = parent.children.borrow();
        let is_backref = |node: &Handle| {
            Self::tag(node).as_deref() == Some("a") && has_class(node, "index-backref")
        };
        let mut ordinal = 0;
        let mut seen = 0;
        for node in siblings.iter() {
            if !is_backref(node) {
                continue;
            }
            seen += 1;
            if Rc::ptr_eq(node, handle) {
                ordinal = seen;
            }
        }
        if ordinal == 0 {
            return None;
        }
        let term: String = siblings
            .iter()
            .filter(|node| !is_backref(node))
            .map(Self::flat_text)
            .collect();
        let term = term.trim();
        if term.is_empty() {
            return None;
        }
        let label = self.label(LABEL_INDEX_BACKREF);
        Some(vec![
            format!("{label} {term}"),
            format!("{label} {term} {ordinal}"),
        ])
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

    /// The step a child of `parent` gets: its element name, or the synthetic
    /// `text()` for a node with none, indexed by its position among ALL of the
    /// parent's child nodes - text nodes included (PART 12 §16).
    ///
    /// Every caller that lifts a child out of the list it walks needs this,
    /// because the index a diagnostic prints belongs to the DOCUMENT and not to
    /// whatever vector the importer happened to build (markup-carve/carve#1554).
    fn child_path(parent: &str, child: &Handle, index: usize) -> String {
        let tag = Self::tag(child).unwrap_or_else(|| "text()".into());
        format!("{parent}/{tag}[{}]", index + 1)
    }

    fn blocks(
        &mut self,
        handles: &[Handle],
        parent: &str,
        depth: usize,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        self.blocks_at(handles, None, parent, depth)
    }

    /// The block walk, with an optional path per handle.
    ///
    /// `paths` is index-parallel to `handles` and says where each node SITS in
    /// the document, which is not always where it sits in `handles`: a caller
    /// that lifted a `<summary>` or a `<figcaption>` out of the child list hands
    /// the rest of it here, and rebuilding an index from the filtered array
    /// renumbered every sibling after the hole (PART 12 §16,
    /// markup-carve/carve#1554).
    fn blocks_at(
        &mut self,
        handles: &[Handle],
        paths: Option<&[String]>,
        parent: &str,
        depth: usize,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let mut out = Vec::new();
        let mut inline = Vec::new();
        // THE PARAGRAPH THIS SYNTHESIZES IS NOT A STEP, so it is not an index
        // basis either. A bare inline run is wrapped here, the wrapper prints no
        // path step - and numbering the run inside the wrapper anyway reported
        // `<p>z</p><kbd onclick="x()">K</kbd>` at `/kbd[1]`, where the `<kbd>` is
        // the SECOND body child. The buffer therefore carries the path each node
        // already has among its siblings, which is the level the step is printed
        // at (PART 12 §16, markup-carve/carve#1554).
        let mut inline_paths: Vec<String> = Vec::new();
        for (i, handle) in handles.iter().enumerate() {
            let tag = Self::tag(handle);
            let is_block = tag.as_deref().map(Self::is_block_tag).unwrap_or(false);
            let path = match paths {
                Some(given) => given[i].clone(),
                None => Self::child_path(parent, handle, i),
            };
            if is_block {
                if !inline.is_empty() {
                    let children =
                        self.inlines_at(&inline, Some(&inline_paths), parent, depth + 1)?;
                    if visible(&children) {
                        out.push(BlockNode::Paragraph(Paragraph {
                            attrs: None,
                            children: trim_edge_whitespace(children),
                            at_content_column: true,
                            pos: None,
                        }));
                    }
                    inline.clear();
                    inline_paths.clear();
                }
                out.extend(self.block(handle, &path, depth + 1)?);
            } else {
                inline.push(handle.clone());
                inline_paths.push(path);
            }
        }
        if !inline.is_empty() {
            let children = self.inlines_at(&inline, Some(&inline_paths), parent, depth + 1)?;
            if visible(&children) {
                out.push(BlockNode::Paragraph(Paragraph {
                    attrs: None,
                    children: trim_edge_whitespace(children),
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
        if tag == FOOTNOTE_PLACEMENT_TAG {
            // AN ENDNOTES SECTION THAT IS NOT LAST KEEPS ITS POSITION.
            //
            // The notes are consumed into `footnote_defs` and the renderer
            // appends the section it rebuilds at DOCUMENT END. That reproduces
            // the input exactly where the section was already last, and
            // silently MOVES it where it was not: the same characters in a
            // different order, with nothing said.
            //
            // This is not `structure-unspellable` and there is nothing to
            // report. Carve HAS a spelling for the position - the
            // `::: footnotes` placement directive - and that is the whole
            // argument: treating placement as a rendering artifact would be
            // defensible only if the language could not say otherwise
            // (markup-carve/carve#1627, docs/html-import.md).
            return Ok(vec![BlockNode::Admonition(Admonition {
                attrs: None,
                kind: "footnotes".to_string(),
                title: None,
                label: None,
                children: Vec::new(),
                pos: None,
            })]);
        }
        if tag == "html" || tag == "head" || tag == "body" {
            // No attribute report here, and deliberately none: `fragment_top_level`
            // strips the scaffold before the walk begins, so an attribute on
            // `<html>` or `<body>` never reaches `attrs` and a check placed
            // here could not fire. That silent drop is older than this policy
            // and is pinned by `unwrapped_wrappers_are_not_a_path` below.
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
            let inlines = self.inlines(&children, path, depth + 1)?;
            // PART 11 §7: a block element holding nothing but LAYOUT builds no
            // node, and the drop is REPORTED because it is a real loss - an
            // element the input had contributes nothing. Keeping owes no report,
            // since a kept character survives the write intact.
            //
            // WHY THE PARAGRAPH AND NOT EVERY BLOCK. §7's argument is that the
            // node left behind is UNSPELLABLE, so the two import exits disagree
            // about it. A heading spells its own marker and a list item its own
            // bullet, so neither is that shape; the paragraph is, and it is the
            // shape all three of §7's rows are written over.
            if is_layout_only(&inlines) {
                self.diag(
                    HtmlImportDiagnosticCode::ElementDropped,
                    format!("Dropped whitespace-only <{tag}> holding no content character"),
                    HtmlImportSeverity::Warning,
                    path,
                    h,
                );
                return Ok(Vec::new());
            }
            return Ok(vec![BlockNode::Paragraph(Paragraph {
                attrs,
                children: inlines,
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
            // A NON-`li` CHILD IS NOT DISCARDED, and it is not discarded in
            // silence either (carve-rs#1261). Filtering the children down to
            // `<li>` and walking only those took the WHOLE of anything else the
            // list carried - `<ul><div id="stray">z</div><li>a</li></ul>` came
            // back as one item and an empty report, so the text `z` left the
            // document with nothing anywhere saying it had.
            //
            // HTML5 does not allow the shape. A sliced-up editor export
            // produces it anyway, and that is the input an importer exists for.
            //
            // The content is emitted as blocks AHEAD OF THE LIST, which is the
            // call `<dd>`-with-no-`<dt>` already made in `definition_list`: it
            // keeps every word and stays valid Carve, where a list holding a
            // non-item has no Carve spelling at all. The stray child goes
            // through the ORDINARY block walk rather than being unwrapped by
            // hand, so it keeps its own element and attributes too - a
            // `<div id="stray">` comes back as a Carve div still carrying the
            // id. Unwrapping it, the way the `<dd>` has to, would drop the id
            // for no reason: a `<dd>` has no standalone spelling and a div has
            // one, so the loss the `<dd>` is forced into is not forced here.
            //
            // `element-unwrapped` is the code: a structural note about the
            // INPUT that loses no meaning, which is what the vocabulary says
            // that code is for. No engine spells "moved", and inventing a code
            // for it is a three-engine decision rather than this defect's.
            //
            // Delegating to `blocks_at` also settles the kinds that are not
            // elements at all: a margin between pretty-printed items is blank
            // text and produces nothing, a `<script>` is dropped with the
            // `element-dropped` every other site gives it, and bare text
            // directly inside the list is wrapped in the paragraph it needs.
            // The paths are the child's OWN indices among the list's children,
            // so the report points where the node sits and not where it sits in
            // the filtered array (PART 12 §16).
            let mut list_items: Vec<Handle> = Vec::new();
            let mut stray: Vec<Handle> = Vec::new();
            let mut stray_paths: Vec<String> = Vec::new();
            for (i, child) in children.iter().enumerate() {
                if Self::tag(child).as_deref() == Some("li") {
                    list_items.push(child.clone());
                    continue;
                }
                let p = Self::child_path(path, child, i);
                if let Some(stray_tag) = Self::tag(child) {
                    // An ACTIVE element is not kept and must not be reported as
                    // if it were: the ordinary walk drops it with the
                    // `element-dropped` every other site gives it, and a
                    // position note beside that would tell the reader the
                    // content survived ahead of the list when it did not.
                    if !matches!(
                        stray_tag.as_str(),
                        "script" | "style" | "template" | "noscript"
                    ) {
                        self.diag(
                            HtmlImportDiagnosticCode::ElementUnwrapped,
                            format!(
                                "A <{stray_tag}> inside <{tag}> kept its content but not its place among the items: it is emitted as blocks ahead of the list"
                            ),
                            HtmlImportSeverity::Warning,
                            &p,
                            child,
                        );
                    }
                } else if !matches!(&child.data, NodeData::Comment { .. })
                    && !Self::text(child).trim().is_empty()
                {
                    self.diag(
                        HtmlImportDiagnosticCode::ElementUnwrapped,
                        format!(
                            "Text directly inside <{tag}> kept its content but not its place among the items: it is emitted as a paragraph ahead of the list"
                        ),
                        HtmlImportSeverity::Warning,
                        &p,
                        child,
                    );
                }
                stray.push(child.clone());
                stray_paths.push(p);
            }
            let mut before = if stray.is_empty() {
                Vec::new()
            } else {
                self.blocks_at(&stray, Some(&stray_paths), path, depth + 1)?
            };
            let mut items = Vec::new();
            for (i, li) in list_items.iter().enumerate() {
                let p = format!("{path}/li[{}]", i + 1);
                items.push(ListItem {
                    attrs: self.attrs(li, &p),
                    checked: None,
                    children: self.blocks(&li.children.borrow(), &p, depth + 1)?,
                    pos: None,
                });
            }
            // Tightness is decided by the ITEM SHAPE, on the SOURCE markup
            // rather than on what the import made of it (carve#1210: "a
            // bare-text <li> imports as a TIGHT list item; <li><p>...</p></li>
            // stays loose. HTML draws the tight/loose distinction the same way
            // Carve does, and import preserves source structure rather than
            // normalizing"). Corpus-convert 27 and 28 are the two halves.
            //
            // Carve spells tightness per LIST, not per item, so a MIXED list
            // has to resolve one way, and it resolves LOOSE - the way
            // CommonMark resolves it, where one paragraph item loosens the
            // whole list. Resolving it tight instead would drop the paragraph
            // that item actually spelled, which is the loss this rule exists to
            // prevent.
            //
            // ONLY A DIRECT `<p>` VOTES, which is what makes the predicate one
            // line instead of a list of exemptions. An item holding only a
            // sublist, only a block quote, only a code block, or nothing at all
            // spells no paragraph, so it does not loosen the list and does not
            // come back with a `<p>` its source never wrote; a nested `<ul>`
            // beside bare text is structure, not a paragraph wrapper, so
            // `<li>one<ul>…</ul></li>` is the HTML of a tight item with a
            // sublist. Asking instead whether every item is BARE TEXT loosens
            // all four of those shapes, which is the over-application
            // markup-carve/carve-js#1106 shipped and markup-carve/carve-js#1110
            // corrected.
            let tight = !list_items.iter().any(|li| {
                li.children
                    .borrow()
                    .iter()
                    .any(|child| Self::tag(child).as_deref() == Some("p"))
            });
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
            before.push(BlockNode::List(List {
                attrs,
                ordered,
                start,
                ol_type,
                bare_marker: false,
                delim: None,
                bullet_char: None,
                tight,
                items,
                pos: None,
            }));
            return Ok(before);
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
            // THE MATH BLOCK FORM. `<div class="math display">\[…\]</div>` is
            // what pandoc and this engine's `MathBlock` extension both write,
            // and it used to import as an ordinary Carve div holding the
            // delimiters as text - the same loss `carve_math` fixes for the
            // inline `<span>`.
            //
            // It comes back as the CORE display form: a paragraph holding one
            // display math node, written `$$` in front of a verbatim span
            // (PART 9 §18, ruled at markup-carve/carve#1514).
            //
            // The block form was left UNIMPLEMENTED here until that ruling,
            // because carve-php took the ```` ```math ```` fence - the fence
            // produced the HTML, so the round trip is exact - and carve-js took
            // the core form. The fence lost: it is an EXTENSION, so wherever
            // that extension is not loaded the same imported document is a
            // `language-math` code block instead of an equation, and an
            // importer must not emit a construct whose meaning depends on the
            // consumer's configuration. `math_display` is core and needs
            // nothing loaded. The round trip is real and is not the job: an
            // importer produces a document that MEANS what the HTML meant, and
            // it cannot know an extension generated the HTML at all.
            if let Some(math) = Self::carve_math(h, attrs.as_ref()) {
                // Charged because this arm returns without walking the children
                // `blocks` would have counted one by one - the same reason the
                // `<math>` arm charges by hand. Which arm an element takes must
                // not change what `max_nodes` and `max_depth` see. Only a div
                // whose classes already carry the pair reaches here, and
                // `carve_math` tests them before it reads any text, so a plain
                // div pays for nothing.
                //
                // At `depth + 1`, which is where the skipped traversal would
                // have started: the ordinary arm below hands the children to
                // `blocks` at `depth + 1`. Charging from `depth` undercounts by
                // one, and a math div then imported at a `max_depth` its own
                // twin was rejected at - measured, not assumed, by the depth
                // half of `the_block_recognition_costs_the_same_budget_as_the_
                // div_it_replaces`.
                self.charge_subtree(h, depth + 1)?;
                return Ok(vec![BlockNode::Paragraph(Paragraph {
                    attrs: None,
                    children: vec![InlineNode::Math(math)],
                    at_content_column: true,
                    pos: None,
                })]);
            }
            if let Some(kind) = Self::container_from(&tag, &attrs) {
                let attrs = Self::without_structural_class(attrs, &kind);
                // A Tier-2 container carries a title the same way a callout
                // does, and renders it with no generated id - so the lift has to
                // reach this arm as well, not only the `<aside>` one below.
                let (title, body, body_paths) = self.admonition_title(&children, path, depth)?;
                let (label, body, body_paths) = self.container_label(body, body_paths, depth)?;
                return Ok(vec![BlockNode::Admonition(Admonition {
                    attrs,
                    kind,
                    title,
                    label,
                    children: self.blocks_at(&body, Some(&body_paths), path, depth + 1)?,
                    pos: None,
                })]);
            }
            let blocks = self.blocks(&children, path, depth + 1)?;
            // AN ATTRIBUTE-LESS DIV IS NOT A CONTAINER WORTH SPELLING, so it
            // unwraps to its content and the `:::` fence is not written
            // (markup-carve/carve#1578). A bare `<div>` carries no meaning of
            // its own: the fence buys the reader nothing and costs them two
            // lines of markup nobody asked for. The element not surviving the
            // round trip is the honest outcome, because there is nothing in it
            // to survive.
            //
            // The BOUNDARY is the whole rule, and it is the attribute rather
            // than the tag: the moment a div carries any attribute the language
            // can hold, the fence comes back, because then there IS something
            // only the container can hold. So the test is `attrs`, not the
            // markup - `<div style="color:red">` keeps nothing after the style
            // map refuses the declaration, and unwraps like any other bare div.
            //
            // Not conditioned on the import MODE either. Roundtrip mode
            // promises the original bytes back for what this engine cannot
            // spell, and an attribute-less div is not such a shape: nothing
            // about it is unspellable, there is simply nothing to spell.
            //
            // Silent, and deliberately: `report_unplaceable_attrs` exists for
            // attributes that lose their carrier, and here there are none by
            // construction. Nothing left the document, so nothing is announced.
            let Some(attrs) = attrs else {
                return Ok(blocks);
            };
            return Ok(vec![BlockNode::Div(Div {
                attrs: Some(attrs),
                label: None,
                children: blocks,
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
            return self.figure_panel(h, path, depth, attrs, true);
        }
        // Any OTHER `<figure>` is a foreign one, and it rebuilds through the
        // same path: its `<figcaption>` becomes the caption line and its content
        // becomes the target. Unwrapped instead, the caption text ran straight
        // onto the content it captioned - `<figure><img><figcaption>cap` came
        // back as `![a](i.png)cap`, one paragraph in which the figure is gone
        // rather than degraded (carve#1286, carve-rs#1027). `figure_panel`
        // already carries the every-target mapping and the multi-block
        // fallback, so a foreign figure gets the ruled shape for free.
        //
        // NOT in roundtrip mode, which promises the original bytes back for
        // anything this engine cannot guarantee, and says so with a
        // `raw-preserved` warning. The rebuild is only lossless for the targets
        // the caption-line syntax re-parses: a figure around a bare PARAGRAPH
        // writes `x` then `^ cap`, which reads back as one paragraph of prose
        // with the caption as literal text, and one around a LIST detaches the
        // caption into a paragraph of its own. Taking this path in roundtrip
        // mode would trade a documented warning for a silent structural loss, so
        // the raw fallback below keeps that mode's contract intact.
        if tag == "figure" && self.opts.mode != HtmlImportMode::Roundtrip {
            let carried = attrs.clone();
            let blocks = self.figure_panel(h, path, depth, attrs, false)?;
            // A rebuild that produced a FIGURE lost nothing: the wrapper became
            // the node and its attributes came with it, so there is nothing to
            // report. Anything else is `figure_panel`'s fallback - no caption to
            // bind, a target the caption line does not attach to, or several
            // body blocks - and there the wrapper and its attributes are gone.
            // That is the same loss the generic path used to announce, so it is
            // announced here too rather than becoming silent (carve#1286).
            // Whether the rebuild is LOSSLESS is a question about the written
            // form, not about the node: `figure_panel` hands back a `Figure` for
            // targets the writer cannot spell as one, and taking the node at
            // face value would make those the only lossy shapes that say
            // nothing. But WHERE it is said matters as much: a target the writer
            // cannot spell is a WRITER loss, not an import one. The AST keeps a
            // proper `Figure` with its attributes, so `html_to_ast` must not be
            // told anything - which is the split PART 12 §16 draws and what
            // `unspellable` is for: those messages surface only when writing.
            // Measured, one figure per target:
            //
            // - image, code block and quote write a caption line the parser
            //   reads back as a figure. Nothing is lost anywhere.
            // - a TABLE writes `^ cap` onto the table, which reads back as a
            //   `<table><caption>`. The caption survives and so do the
            //   attributes, but on the TABLE, so only the wrapper is gone.
            // - a PARAGRAPH writes `x` then `^ cap`, which reads back as one
            //   paragraph of prose: the caption line does not attach to prose,
            //   so the wrapper and its attributes both go.
            //
            // Anything that is NOT a single figure is a real IMPORT loss - the
            // AST has no figure either - so it stays a plain diagnostic.
            match blocks.as_slice() {
                [BlockNode::Figure(f)] => {
                    let lost = match *f.target {
                        FigureTarget::Table(_) => Some(
                            "A <figure> around a table has no Carve spelling: the written table \
                             carries the caption and the figure's attributes, and reads back as a \
                             captioned table rather than a figure",
                        ),
                        FigureTarget::Paragraph(_) => Some(
                            "A <figure> around a paragraph has no Carve spelling: the written \
                             caption line does not attach to prose, so it reads back as one \
                             paragraph and the figure and its attributes are gone",
                        ),
                        _ => None,
                    };
                    if let Some(message) = lost {
                        self.unspellable
                            .push((h.clone(), path.to_owned(), message.into()));
                    }
                }
                _ => {
                    self.diag(
                        HtmlImportDiagnosticCode::ElementUnwrapped,
                        format!("Unwrapped unsupported <{tag}> element"),
                        HtmlImportSeverity::Info,
                        path,
                        h,
                    );
                    self.report_unplaceable_attrs(
                        h,
                        carried,
                        tag.as_str(),
                        "the figure did not survive as a figure",
                        path,
                    );
                }
            }
            return Ok(blocks);
        }
        // A RENDERED CALLOUT, before the unwrap claims it. `<aside>` reaches
        // here rather than the `<div>` arm above, and unwrapping it dropped the
        // `Admonition` node outright - the construct did not degrade, it left.
        if let Some(kind) = Self::container_from(&tag, &attrs) {
            let attrs = Self::without_structural_class(
                Self::without_structural_class(attrs, "admonition"),
                &kind,
            );
            let (title, body, body_paths) = self.admonition_title(&children, path, depth)?;
            let (label, body, body_paths) = self.container_label(body, body_paths, depth)?;
            return Ok(vec![BlockNode::Admonition(Admonition {
                attrs,
                kind,
                title,
                label,
                children: self.blocks_at(&body, Some(&body_paths), path, depth + 1)?,
                pos: None,
            })]);
        }
        if self.opts.mode == HtmlImportMode::Roundtrip {
            self.diag(
                HtmlImportDiagnosticCode::RawPreserved,
                format!("Preserved unsupported <{tag}> element as raw HTML"),
                HtmlImportSeverity::Warning,
                path,
                h,
            );
            return Ok(vec![BlockNode::RawBlock(RawBlock {
                format: "html".into(),
                content: Self::html(h),
                pos: None,
            })]);
        }
        // A DERIVED WRAPPER IS NOT A LOSS. Unwrapping the endnotes `<section>`
        // removes nothing an author wrote - no Carve construct spells a
        // `<section>`, and PART 9 §16 is what put this one here - so PART 9
        // §16a reports it neither as `element-unwrapped` nor as an
        // `attribute-dropped` naming the role or the derived name.
        //
        // Only the ELEMENT row goes. The attribute rows below still run, so an
        // authored `class`, or an `aria-label` no default matches, is reported
        // the way it would be on any other unwrap: the two suppressions
        // together would silence the author's own attributes with the
        // renderer's, which is the failure the clause calls out by name.
        //
        // The import's OUTCOME does not enter into it. This is the
        // reference-less form, which degrades to the `<hr>` and `<ol>` it is
        // built from and gets no section written back; a referenced one is
        // consumed into footnote definitions instead. Derivation is a property
        // of the element being READ, so both answer the same.
        if !self.is_derived_wrapper(h, &tag) {
            self.diag(
                HtmlImportDiagnosticCode::ElementUnwrapped,
                format!("Unwrapped unsupported <{tag}> element"),
                HtmlImportSeverity::Info,
                path,
                h,
            );
        }
        // AN UNWRAPPED ELEMENT TAKES ITS ATTRIBUTES WITH IT. `<section
        // role="region">`, `<article>`, `<aside>`, `<main>`, `<nav>` and
        // `<form>` all land here and keep only their children. The keep list
        // refused most of these names inside `attrs` and reported them there,
        // so widening retention without this would have converted a reported
        // loss into a silent one - the opposite of what carve-rs#1060 asks for.
        self.report_unplaceable_attrs(
            h,
            attrs,
            tag.as_str(),
            "the element was unwrapped and has no node to carry it",
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
    /// A rebuilt callout's TITLE, lifted out of its body.
    ///
    /// `::: note "A"` renders the title as a `<p class="admonition-title">`
    /// inside the aside, so the rebuild has to take it back out: left in the
    /// body it is written back as an ordinary paragraph carrying the renderer's
    /// own class, which renders a SECOND title element on the next pass and
    /// makes the callout's opening line ordinary prose.
    ///
    /// A TITLE HOLDS INLINE CONTENT AND HAS NO ATTRIBUTE SLOT, so what the
    /// paragraph carried cannot come with it. It is REPORTED rather than
    /// dropped in silence, and reported rather than tucked into a span the way
    /// `<summary>` carries its own: carve-php and carve-js both answer this
    /// construct with a diagnostic, and a construct the three engines are being
    /// converged on is the wrong place to keep a fourth answer. The structural
    /// `admonition-title` class is consumed rather than reported - the renderer
    /// writes it back from the title, exactly as it writes the container's own
    /// `admonition` class back from the kind.
    ///
    /// Returns the title, and the body with the paths its children ARRIVED
    /// under: filtering the title out and renumbering pulls every sibling after
    /// it one step forward, which is the defect carve#1554 fixed for
    /// `<summary>` (PART 12 §16).
    #[allow(clippy::type_complexity)]
    fn admonition_title(
        &mut self,
        children: &[Handle],
        path: &str,
        depth: usize,
    ) -> Lifted<Vec<InlineNode>> {
        let at = children.iter().position(is_admonition_title);
        let mut title = None;
        if let Some(i) = at {
            let title_path = Self::child_path(path, &children[i], i);
            // The element itself, not only its children: an empty title is a
            // DOM node the caller's `max_nodes` is counting, and reading past it
            // let a document process more nodes than the limit allows.
            self.enter(depth + 1)?;
            let mut own = self.attrs(&children[i], &title_path);
            if let Some(attrs) = own.as_mut() {
                attrs.classes.retain(|class| class != "admonition-title");
                attrs.order.retain(|slot| *slot != AttrSlot::Class);
            }
            if let Some(attrs) =
                own.filter(|a| a.id.is_some() || !a.classes.is_empty() || !a.key_values.is_empty())
            {
                let names = Self::attr_names(&attrs).join(", ");
                self.diag(
                    HtmlImportDiagnosticCode::AttributeDropped,
                    format!("Dropped {names} on <p>: an admonition title has no attribute slot"),
                    HtmlImportSeverity::Warning,
                    &title_path,
                    &children[i],
                );
            }
            title = Some(self.inlines(&children[i].children.borrow(), &title_path, depth + 2)?);
        }
        let mut body = Vec::new();
        let mut body_paths = Vec::new();
        for (i, child) in children.iter().enumerate() {
            if Some(i) == at {
                continue;
            }
            body_paths.push(Self::child_path(path, child, i));
            body.push(child.clone());
        }
        Ok((title, body, body_paths))
    }

    /// PART 9 §10's grouping `[label]`, taken back off the paragraph the
    /// renderer degraded it to.
    ///
    /// The renderer surfaces an unconsumed label as `<p class="div-label">`, so
    /// that an extension nobody loaded does not swallow what the author wrote.
    /// Importing it as an ordinary paragraph is render-neutral on every
    /// container but ONE - and that one is the reason this exists. `::: figure`
    /// with NO title and NO label is a composite figure (§4c), so moving the
    /// label off the opener changed the ELEMENT: `::: figure [g]` rendered
    /// `<div class="figure">` and came back as `<figure
    /// class="carve-figure-group">` (markup-carve/carve-rs#1310).
    ///
    /// The title has had this lift since it was written; the label never got
    /// one, and the asymmetry is the whole defect. It also ends a second loss
    /// on every container: a label is RAW, and a paragraph is not, so
    /// `::: figure [a *b*]` came back with the asterisk escaped and said
    /// something new on each format pass.
    ///
    /// THE FIRST ELEMENT ONLY, unlike the title, which is lifted from wherever
    /// it stands. The renderer writes the label immediately after the title and
    /// before the body, so first is where its own output puts it - and a
    /// paragraph found further down would be MOVED to the opener rather than
    /// recognized, which changes a document instead of restoring one.
    fn container_label(
        &mut self,
        body: Vec<Handle>,
        body_paths: Vec<String>,
        depth: usize,
    ) -> Lifted<String> {
        let Some(at) = body.iter().position(|child| Self::tag(child).is_some()) else {
            return Ok((None, body, body_paths));
        };
        if !is_div_label(&body[at]) {
            return Ok((None, body, body_paths));
        }
        // TEXT ONLY. The field is a raw `String` and the writer emits it raw, so
        // lifting a paragraph holding markup would flatten the markup and lose
        // it without a word.
        let element_children = body[at].children.borrow();
        if element_children.iter().any(|c| Self::tag(c).is_some()) {
            return Ok((None, body.clone(), body_paths));
        }
        let text = Self::text(&body[at]);
        drop(element_children);
        // A LABEL HOLDING `]` OR A LINE BREAK HAS NO SPELLING. Every reader of
        // this run takes it up to the first `]`, with no balance and no escape
        // (see `write_flat_bracket_run`), so writing one back would not read as
        // a label at all - it would take the container's own opener line with
        // it. Left as a paragraph, which is what it already is.
        if text.contains(']') || text.contains('\n') {
            return Ok((None, body, body_paths));
        }
        let label_path = body_paths[at].clone();
        // The element itself, for the same reason the title charges for its
        // own: it is a DOM node `max_nodes` is counting, and a lift that reads
        // past it without charging lets a document process more than the limit.
        self.enter(depth + 1)?;
        let mut own = self.attrs(&body[at], &label_path);
        if let Some(attrs) = own.as_mut() {
            attrs.classes.retain(|class| class != "div-label");
            attrs.order.retain(|slot| *slot != AttrSlot::Class);
        }
        if let Some(attrs) =
            own.filter(|a| a.id.is_some() || !a.classes.is_empty() || !a.key_values.is_empty())
        {
            let names = Self::attr_names(&attrs).join(", ");
            let node = body[at].clone();
            self.diag(
                HtmlImportDiagnosticCode::AttributeDropped,
                format!("Dropped {names} on <p>: a container label has no attribute slot"),
                HtmlImportSeverity::Warning,
                &label_path,
                &node,
            );
        }
        let mut rest = Vec::with_capacity(body.len() - 1);
        let mut rest_paths = Vec::with_capacity(body_paths.len() - 1);
        for (i, (child, child_path)) in body.into_iter().zip(body_paths).enumerate() {
            if i == at {
                continue;
            }
            rest.push(child);
            rest_paths.push(child_path);
        }
        Ok((Some(text), rest, rest_paths))
    }

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
        // The summary comes OUT of the list, and nothing else moves: rebuilding
        // an index from the filtered vector pulled every sibling after it one
        // step forward, so `<details><summary>s</summary><p onclick>` reported
        // `/details[1]/p[1]` for the second child (PART 12 §16,
        // markup-carve/carve#1554).
        let mut body: Vec<Handle> = Vec::new();
        let mut body_paths: Vec<String> = Vec::new();
        for (i, child) in children.iter().enumerate() {
            if Some(i) == summary {
                continue;
            }
            body_paths.push(Self::child_path(path, child, i));
            body.push(child.clone());
        }
        Ok(Admonition {
            attrs,
            kind: "details".into(),
            title,
            label: None,
            children: self.blocks_at(&body, Some(&body_paths), path, depth + 1)?,
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
            h,
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
        // EACH ENTRY CARRIES ITS OWN PATH, not its position in this vector. The
        // walk collects only `<dt>` and `<dd>`, so a numbering rebuilt from the
        // collection counted among the entries rather than among the `<dl>`'s
        // children: a pretty-printed list reported its second description at
        // `/dl[1]/dd[2]` where the whitespace text nodes make it the fourth
        // child, and an entry reached through a `<div>` group wrapper lost the
        // wrapper's step entirely (PART 12 §16, markup-carve/carve#1554).
        let mut entries: Vec<(String, Handle, usize, String)> = Vec::new();
        for (i, child) in h.children.borrow().iter().enumerate() {
            let Some(tag) = Self::tag(child) else {
                continue;
            };
            self.enter(depth + 1)?;
            match tag.as_str() {
                "dt" | "dd" => {
                    let p = Self::child_path(path, child, i);
                    entries.push((tag, child.clone(), depth + 1, p));
                }
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
                            child,
                        );
                    }
                    for (j, wrapped) in child.children.borrow().iter().enumerate() {
                        let Some(inner) = Self::tag(wrapped) else {
                            continue;
                        };
                        self.enter(depth + 2)?;
                        if inner == "dt" || inner == "dd" {
                            let inner_path = Self::child_path(&p, wrapped, j);
                            entries.push((inner, wrapped.clone(), depth + 2, inner_path));
                        } else {
                            // One level unwraps, which is the only level HTML5
                            // allows, so a `div` inside the wrapper is not a
                            // group. It is still reported: a doubly-wrapped
                            // list otherwise imports to nothing at all, which
                            // is the silent shape this row exists to remove.
                            self.dropped_in_dl(wrapped, &inner, &format!("{p}/{inner}[{}]", j + 1));
                        }
                    }
                }
                other => self.dropped_in_dl(child, other, &format!("{path}/{other}[{}]", i + 1)),
            }
        }

        let mut before: Vec<BlockNode> = Vec::new();
        let mut items: Vec<DefinitionItem> = Vec::new();
        let mut terms: Vec<DefinitionTerm> = Vec::new();
        let mut definitions: Vec<DefinitionDef> = Vec::new();
        for (tag, node, node_depth, entry_path) in entries.iter() {
            let p = entry_path.clone();
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
                    node,
                );
                // AND IT LOSES ITS ATTRIBUTES WITH ITS ROLE (carve-rs#1257).
                // Every other `<dd>` puts them on the `DefinitionDef` below;
                // this one has no node to carry them, and reading only its
                // CHILDREN meant an `onclick` here went the way the table
                // caption's did - stripped, with the row above naming the role
                // and nothing naming the attribute. Same category, second site.
                let carried = self.attrs(node, &p);
                self.report_unplaceable_attrs(
                    node,
                    carried,
                    "dd",
                    "a <dd> with no <dt> keeps its content as blocks, and blocks ahead of the list have no slot for it",
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
            // The importer reads the `<dd>` wrappers it was given, and a
            // description that already holds a block says its own looseness -
            // so nothing here is the SPELLED fact §17 L7's field records.
            loose: false,
            pos: None,
        }));
        Ok(before)
    }
    fn dropped_in_dl(&mut self, node: &Handle, tag: &str, path: &str) {
        self.diag(
            HtmlImportDiagnosticCode::ElementDropped,
            format!("Dropped <{tag}> inside <dl>: only <dt>, <dd> and a single <div> group wrapper are definition-list content"),
            HtmlImportSeverity::Warning,
            path,
            node,
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

    /// The container an `<aside>` or `<div>` was RENDERED FROM, rebuilt.
    ///
    /// This is `render_admonition` read backwards, and it is written as that
    /// inverse rather than as a list of names on purpose. The renderer sends an
    /// `Admonition` to exactly two shapes: a kind in `ADMONITION_TIER1_KINDS`
    /// becomes `<aside class="admonition {kind}">`, and every other kind - a tab
    /// set, a code group, a panel, a Tier-2 container an extension invented -
    /// becomes `<div class="{kind}">`, with the node's own extra classes
    /// appended after the structural one. Inverting the mapping therefore covers
    /// the containers nobody has thought of yet; naming `tabs` and `code-group`
    /// would have covered two and gone on losing the rest (carve-rs#1240).
    ///
    /// WHAT IT COSTS TO UNWRAP INSTEAD is a node, not bytes, which is why an
    /// HTML-to-HTML check never found it: an unwrapped `<aside>` re-renders as
    /// the same `<p>` it went in as, and a `<div class="tabs">` kept as a `Div`
    /// carrying a `.tabs` class re-renders byte-identically too. Only the AST
    /// moved - `Admonition` became `Div`, or vanished - and the document stopped
    /// being a callout while looking exactly like one (carve-js#1295).
    ///
    /// THE STRUCTURAL CLASS IS CONSUMED into the fence word rather than kept
    /// beside it, because the renderer writes it back from the kind: keeping it
    /// would emit `class="tabs tabs"` on the next render, and the derived-name
    /// rule already reads the same class to recognize the naming attributes
    /// these elements carry.
    fn container_from(tag: &str, attrs: &Option<Attrs>) -> Option<String> {
        let classes = attrs.as_ref().map(|a| a.classes.as_slice()).unwrap_or(&[]);
        let kind = match tag {
            // The class PAIR is what marks a rendered callout. A bare `<aside>`
            // is somebody else's sidebar and keeps the unwrap it has always had.
            "aside" => {
                if !classes.iter().any(|c| c == "admonition") {
                    return None;
                }
                classes
                    .iter()
                    .find(|c| {
                        c.as_str() != "admonition" && ADMONITION_TIER1_KINDS.contains(&c.as_str())
                    })
                    .cloned()?
            }
            "div" => classes.first().cloned()?,
            _ => return None,
        };
        // The writer's own rule, not a copy of it: a class a fence opener cannot
        // spell (`2col`, `my.class`) would be written after the colons and read
        // back as a paragraph, so such an element keeps the generic `Div` node
        // where the class survives as a class.
        crate::render_carve::is_container_kind(&kind).then_some(kind)
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

    /// A CAPTION ELEMENT's inlines, charged and diagnosed like any other
    /// element - `<figcaption>` on a figure, `<caption>` on a table.
    ///
    /// A caption line has NO attribute slot, so whatever the element carried
    /// cannot come with it. Routing through `attrs` anyway is what keeps the
    /// importer honest: the event-handler and `style` diagnostics fire from
    /// there, and anything still standing afterwards is reported as dropped
    /// rather than vanishing. Reading the caption's children WITHOUT this
    /// silently discarded an `onclick` - the one attribute whose loss a reader
    /// most needs told - and skipped the element's own charge against
    /// `max_nodes` and one level of `max_depth` (carve#1286).
    ///
    /// THE CATEGORY IS "READ FOR ITS CHILDREN", NOT THE TAG NAME
    /// (carve-rs#1257). The table `<caption>` is the same shape reached by a
    /// different route - the row walk looks only for `tr`, so the caption was
    /// lifted out by hand and its children read straight off it - and it was
    /// the one caption site in this importer still dropping an `onclick` in
    /// silence while carve-php and carve-js both reported it. Widening this
    /// helper by one parameter is what makes the next caption slot inherit the
    /// report instead of having to remember it; a branch for `<caption>` would
    /// have fixed the reported input and left the category open.
    fn caption_inlines(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        tag: &str,
    ) -> Result<Vec<InlineNode>, HtmlImportError> {
        self.enter(depth)?;
        let carried = self.attrs(h, path);
        self.report_unplaceable_attrs(
            h,
            carried,
            tag,
            "a caption line carries no attributes",
            path,
        );
        self.inlines(&h.children.borrow(), path, depth + 1)
    }

    /// Whether an inline run carries nothing a reader would see.
    ///
    /// Asked STRUCTURALLY, of the node list itself, rather than by flattening to
    /// text: a flattener answers only for the node kinds it has an arm for, and
    /// the one in the renderer has none for `Span`, so a caption reading
    /// `<span class="label">Caption</span>` flattened to the empty string and
    /// this call would have thrown the caption away. Emptiness must not depend
    /// on another function's coverage, so only a whitespace-only text node
    /// counts as blank and every other node counts as content.
    fn inlines_are_blank(nodes: &[InlineNode]) -> bool {
        nodes.iter().all(|n| match n {
            InlineNode::Text(t) => t.value.trim().is_empty(),
            _ => false,
        })
    }

    /// Report attributes that survived [`Self::attrs`] but have nowhere to go.
    ///
    /// `attrs` diagnoses what it REFUSES (event handlers, unmapped CSS) and
    /// hands back what it accepted. When the caller then has no slot for the
    /// result, staying quiet would report the refused attributes and swallow the
    /// accepted ones, which is the wrong way round: the reader is told about the
    /// `onclick` and not about the `id` that also went missing (carve#1286).
    fn report_unplaceable_attrs(
        &mut self,
        node: &Handle,
        attrs: Option<Attrs>,
        tag: &str,
        because: &str,
        path: &str,
    ) {
        let Some(attrs) = attrs else {
            return;
        };
        let mut dropped: Vec<String> = Vec::new();
        if let Some(id) = attrs.id {
            dropped.push(format!("id=\"{id}\""));
        }
        dropped.extend(attrs.classes.iter().map(|c| format!("class=\"{c}\"")));
        dropped.extend(attrs.key_values.keys().map(|k| k.to_string()));
        for name in dropped {
            self.diag(
                HtmlImportDiagnosticCode::AttributeDropped,
                format!("Dropped {name} on <{tag}>: {because}"),
                HtmlImportSeverity::Info,
                path,
                node,
            );
        }
    }

    /// `<figure class="carve-figure-group">` back to the `figure_group` node
    /// it rendered from (PART 9 §4c). The panels nest DIRECTLY, so the group's
    /// own caption is its LAST direct `<figcaption>` child - a panel's
    /// figcaption sits a level down inside the panel figure and is never read
    /// as the group's - and everything else is the child list, each panel
    /// routed back through [`Self::figure_panel`].
    fn figure_group(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let attrs = Self::without_structural_class(attrs, "carve-figure-group");
        let nodes = h.children.borrow();
        let caption_at = nodes
            .iter()
            .rposition(|c| Self::tag(c).as_deref() == Some("figcaption"));
        let mut caption = None;
        if let Some(i) = caption_at {
            let p = format!("{path}/figcaption[{}]", i + 1);
            caption = Some(self.caption_inlines(&nodes[i], &p, depth + 1, "figcaption")?);
        }
        let mut body: Vec<Handle> = Vec::new();
        let mut body_paths: Vec<String> = Vec::new();
        for (i, child) in nodes.iter().enumerate() {
            if Some(i) == caption_at {
                continue;
            }
            // Lifting the caption out is not a renumbering of everything after
            // it (markup-carve/carve#1554).
            body_paths.push(Self::child_path(path, child, i));
            body.push(child.clone());
        }
        // The children list is read out before the walk, so nothing below runs
        // while this node's `RefCell` is still borrowed.
        drop(nodes);
        let children = self.blocks_at(&body, Some(&body_paths), path, depth + 1)?;
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
    ///
    /// `own_output` says whether the wrapper was THIS engine's panel, in which
    /// case the `carve-figure-panel` class is structural and comes back off. A
    /// FOREIGN figure keeps every class it carried: only a class in FIRST
    /// position identifies an own-output panel above, so `class="custom
    /// carve-figure-panel"` arrives here as somebody else's markup, and stripping
    /// the name out of it would silently edit an author's class list
    /// (carve#1286).
    fn figure_panel(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
        own_output: bool,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let attrs = if own_output {
            Self::without_structural_class(attrs, "carve-figure-panel")
        } else {
            attrs
        };
        let mut caption = None;
        // EACH HOST CHILD KEEPS ITS OWN POSITION. The caption is lifted out of
        // the child list here, and every margin the branches below drop leaves
        // another hole in it, so an index rebuilt from this vector names a
        // different node than the one the diagnostic is about (PART 12 §16,
        // markup-carve/carve#1554).
        let mut host: Vec<(usize, Handle)> = Vec::new();
        for (index, child) in h.children.borrow().iter().enumerate() {
            if Self::tag(child).as_deref() == Some("figcaption") {
                // The first NON-BLANK one captions. HTML allows at most one, so
                // a second is somebody's malformed markup - but overwriting the
                // caption with it threw the first one's text away entirely,
                // which is content loss rather than a structural downgrade.
                // Extras fall through to the host instead, where the generic
                // unwrap keeps their text and reports it (carve#1286).
                //
                // Non-blank rather than merely first, because a blank caption is
                // treated as absent below: if the first one is blank, a later
                // one is the only caption the figure has, and captioning with it
                // keeps a visible string in the role its author gave it instead
                // of demoting it to content. No text is lost either way.
                if caption.is_none() {
                    // Wherever the author put it - a literal step with no index
                    // at all said nothing about which `<figcaption>` this was,
                    // and no other step in a path is spelled that way.
                    let p = format!("{path}/figcaption[{}]", index + 1);
                    let inlines = self.caption_inlines(child, &p, depth + 1, "figcaption")?;
                    // An EMPTY caption is not a caption. Kept as one it wrote a
                    // bare `^` line, which re-parses as a literal caret in a
                    // paragraph: the figure destroyed AND a character in the
                    // output the author never typed. Treated as absent it takes
                    // the no-caption path, which writes the content alone and
                    // reports the wrapper as unwrapped (carve#1286). carve-js
                    // writes the bare `^`, so this diverges from it knowingly -
                    // matching it would mean reproducing the corruption.
                    if !Self::inlines_are_blank(&inlines) {
                        caption = Some(inlines);
                    }
                    continue;
                }
            }
            host.push((index, child.clone()));
        }
        // Pretty-printed margins between the wrapper and its host. Kept, they
        // lead the rebuilt image paragraph with a space, and the writer's
        // indented image line then re-parses as prose.
        //
        // LEADING and TRAILING whitespace in a container is insignificant, and
        // that is all a margin ever is. Whitespace BETWEEN siblings is a word
        // boundary the reader can see, whether or not it carries a newline -
        // HTML collapses either to one space - so dropping it joined a foreign
        // `<span>a</span> <span>b</span>` into `ab` (carve#1286). Own output
        // puts nothing but margins here, so it keeps the blunter rule.
        let is_blank_text = |n: &Handle| match &n.data {
            NodeData::Text { contents } => contents.borrow().trim().is_empty(),
            _ => false,
        };
        if own_output {
            host.retain(|(_, c)| !is_blank_text(c));
        } else {
            // A COMMENT is invisible, so a margin does not stop being a margin
            // for sitting on the far side of one. Trimming only text left
            // `<figure><!--x--> <img>` holding a stray space beside the image,
            // which made the host a paragraph rather than an image - the output
            // was unchanged but the figure was then reported as unwrapped when
            // it had not been (carve#1286).
            let is_margin =
                |n: &Handle| is_blank_text(n) || matches!(&n.data, NodeData::Comment { .. });
            let is_margin = |c: &(usize, Handle)| is_margin(&c.1);
            // Found once and drained once. Removing from the front one node at
            // a time shifts the rest on every step, which is quadratic in the
            // number of leading margins - and margins are attacker-controlled
            // and cost nothing to repeat, so the bound has to be reached by
            // charging them rather than by grinding through them.
            let lead = host.iter().take_while(|c| is_margin(c)).count();
            host.drain(..lead);
            let tail = host.iter().rev().take_while(|c| is_margin(c)).count();
            host.truncate(host.len() - tail);
            let trimmed = lead + tail;
            // A node dropped here never reaches `blocks`, so it would never be
            // charged - and a margin is the cheapest thing an author can repeat.
            // Discarding it is not the same as never having walked it, so the
            // budget is spent either way and `<figure>` gains no free ride the
            // rest of the importer does not give.
            for _ in 0..trimmed {
                self.enter(depth + 1)?;
            }
        }
        let host_paths: Vec<String> = host
            .iter()
            .map(|(index, child)| Self::child_path(path, child, *index))
            .collect();
        let host: Vec<Handle> = host.into_iter().map(|(_, child)| child).collect();
        let mut blocks = self.blocks_at(&host, Some(&host_paths), path, depth + 1)?;
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
                target: Box::new(target),
                rendered_target: None,
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
        trs: &[(Handle, Option<usize>)],
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
                valign: None,
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
                            valign: None,
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
                    &trs[r].0,
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
    fn column_groups(h: &Handle, path: &str) -> Vec<(Handle, String)> {
        h.children
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, c)| Self::tag(c).as_deref() == Some("colgroup"))
            .map(|(i, c)| (c.clone(), format!("{path}/colgroup[{}]", i + 1)))
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
        for (i, c) in captions.iter().skip(1) {
            self.diag(
                HtmlImportDiagnosticCode::TableDegraded,
                "Dropped a second <caption>: a table has one caption, and the first one wins"
                    .into(),
                HtmlImportSeverity::Warning,
                &format!("{path}/caption[{}]", i + 1),
                c,
            );
        }
        // THE ELEMENT, not its children (carve-rs#1257). Reading `children` off
        // it here meant the `<caption>` was the only caption slot in this
        // importer whose own attributes were never looked at, so an `onclick`
        // on it was stripped with nothing said - the failure mode the report
        // exists to prevent, and the one both other engines already reported.
        // `caption_inlines` is where the answer already lived.
        let caption_node: Option<(usize, Handle)> = captions.first().map(|(i, c)| (*i, c.clone()));
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
        for (cg, p) in Self::column_groups(h, path) {
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                "Dropped <colgroup>: Carve has no column model, and a table's columns are only the cells its rows carry".into(),
                HtmlImportSeverity::Warning,
                &p,
                &cg,
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
                        cell,
                    );
                    rowspan = leading_header_rows - r;
                }
                row.push(BuiltCell {
                    cell: TableCell {
                        header: Self::tag(cell).as_deref() == Some("th"),
                        span: None,
                        align: None,
                        valign: None,
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
        let mut result = self.span_grid(&trs, built, &row_attrs, path, depth)?;

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
            h,
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
                &section_nodes[id].0,
            );
        }
        // `depth` rather than `depth + 1`: the caption's INLINES stay at the
        // depth the cells are read at, and the element itself takes the level
        // and the node charge it always should have had.
        // THE CAPTION IS NUMBERED WHERE THE AUTHOR PUT IT (PART 12 §16,
        // markup-carve/carve#1560). A step counts among ALL of the parent's
        // child nodes, and the clause's three exemptions - an item among the
        // items, a row among the rows, a cell among the cells of its row - are
        // the whole of it, because the importer reads those parents through a
        // shape of its own. A table has at most one caption, so there is
        // nothing to renumber and no exemption to claim.
        //
        // The literal `caption[1]` this replaces never consulted a position at
        // all, and what it printed was the caption's rank among the captions -
        // the one basis the clause forbids, and the reading a reader also gets
        // from resolving the path as XPath. It agreed with the child index only
        // for a table written with no whitespace: `<table>` on its own line
        // puts a text node first, so the caption is the SECOND child and
        // `caption[1]` named a node the reader does not have. The
        // second-caption row above already counted this way, so one element
        // spoke under two bases.
        let caption = match caption_node {
            Some((index, node)) => Some(self.caption_inlines(
                &node,
                &format!("{path}/caption[{}]", index + 1),
                depth,
                "caption",
            )?),
            None => None,
        };
        Ok(Table {
            attrs,
            caption,
            short_caption: None,
            columns: Vec::new(),
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
    // The table element joins the six it already took, so that the one
    // diagnostic in here can report at the element it is about rather than at
    // whatever the walk was holding.
    #[allow(clippy::too_many_arguments)]
    fn row_groups(
        &mut self,
        node: &Handle,
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
                node,
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
            node.clone(),
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
        self.inlines_at(handles, None, parent, depth)
    }

    /// The inline walk, with the same index-parallel path override
    /// [`Self::blocks_at`] takes, and for the same reason.
    fn inlines_at(
        &mut self,
        handles: &[Handle],
        paths: Option<&[String]>,
        parent: &str,
        depth: usize,
    ) -> Result<Vec<InlineNode>, HtmlImportError> {
        let mut out = Vec::new();
        // A FLATTEN PRESERVES THE BOUNDARY IT DISSOLVES (PART 11 §1b,
        // markup-carve/carve#1325). A slot that takes INLINE content only - a
        // caption line, a fence title, a table cell, an image's alternative
        // text, a definition term - cannot carry blocks, so a producer handed
        // block content for one flattens it. The flatten is lossy by
        // construction and §1's round-trip invariant is not the rule that
        // applies; what the producer still owes is the BOUNDARY.
        //
        // THE UNIT IS THE TOKEN, NOT THE NODE, and that difference is the whole
        // rule: `onetwo` and `one two` are both a single `text` node, and the
        // boundary a reader recovers from one and not the other is a token
        // boundary. Two blocks joined with nothing lost it as CONTENT
        // (`<p>one</p><p>two</p>` -> `onetwo`) and as STRUCTURE
        // (`*a**b*` re-reads as one strong run holding a literal asterisk;
        // two code spans re-read as one holding the joined delimiters).
        //
        // ONE SPACE, at every such boundary, conditioned on nothing further. A
        // space ends a word and ends a delimiter run, and is neither punctuation
        // nor alphanumeric, so it cannot combine with either side into a
        // construct neither block wrote. Conditioning it on the neighbouring
        // characters is the source-pattern test §1a already refuses.
        //
        // A BLOCK THAT CONTRIBUTES NOTHING IS NOT A SIDE, which is why the state
        // below is over CONTRIBUTED content rather than over sibling positions:
        // `<p>a</p><p></p><p>b</p>` holds three blocks and one join, so the slot
        // is `a b` and not `a  b`.
        //
        // INTER-ELEMENT WHITESPACE IS LAYOUT, NOT A TOKEN. A pretty-printed
        // document puts a newline and an indent between the blocks, and those
        // runs contribute nothing to the slot - so inside a flatten they are
        // dropped rather than kept beside the separator, which would emit the
        // `a  b` this clause explicitly refuses one case over. Only inside a
        // flatten: between two INLINE siblings the same run IS content, and
        // `<strong>a</strong> <em>b</em>` must keep its space.
        let flattening = handles
            .iter()
            .any(|h| Self::tag(h).as_deref().is_some_and(Self::is_block_tag));
        let mut published = false;
        let mut boundary_pending = false;
        for (i, h) in handles.iter().enumerate() {
            let tag = Self::tag(h).unwrap_or_else(|| "text()".into());
            let path = match paths {
                Some(given) => given[i].clone(),
                None => format!("{parent}/{tag}[{}]", i + 1),
            };
            // `tag` is already the element's name, or the synthetic `text()` for a
            // node that has none - and that is not a block tag, so this needs no
            // second call to build the same String again.
            let is_block = Self::is_block_tag(&tag);
            if is_block && published {
                boundary_pending = true;
            }
            let produced = self.inline(h, &path, depth)?;
            let contributes = !Self::inlines_are_blank(&produced);
            if !contributes && flattening {
                continue;
            }
            if contributes && boundary_pending {
                out.push(InlineNode::text(" ".to_string()));
            }
            out.extend(produced);
            if contributes {
                published = true;
                boundary_pending = is_block;
            }
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
        // A site the adapter footnote pass recognized. The pass runs before
        // this walk and records the node rather than rewriting the tree into
        // a synthetic element, so nothing here depends on a tag name that
        // real HTML could also spell.
        if let Some((_, label)) = self.footnote_refs.get(&node_key(h)) {
            return Ok(vec![InlineNode::Footnote(Footnote {
                attrs: None,
                id: Some(label.clone()),
                inline: None,
                number: None,
                ref_id: None,
                pos: None,
            })]);
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
                h,
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
                //
                // `encoding-assumed` is the code because the loss is in the
                // OUTPUT: MathML never states what `alttext` holds, so the
                // math node may carry something that is not TeX at all.
                // `element-unwrapped` would describe a structural event the
                // consumer cannot act on (carve#1235).
                if tier == 2 {
                    self.diag(
                        HtmlImportDiagnosticCode::EncodingAssumed,
                        "Read <math> through its alttext: MathML does not declare the encoding of alttext, so TeX is assumed".into(),
                        HtmlImportSeverity::Info,
                        path,
                        h,
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
                    h,
                );
                return Ok(Vec::new());
            }
        }
        let attrs = self.attrs(h, path);
        let children = self.inlines(&h.children.borrow(), path, depth + 1)?;
        if tag == "span" {
            // AFTER the children have been walked, which is what keeps the
            // budget honest for free: `inlines` has already charged every node
            // under this element against `max_nodes` and `max_depth`, so which
            // arm the span takes cannot change what the limits see. The
            // `<math>` arm above has to call `charge_subtree` by hand for the
            // same reason - it returns without walking - and a recognition that
            // read text recursively BEFORE charging would let crafted HTML
            // reach the stack ahead of the limit meant to stop it.
            if let Some(math) = Self::carve_math(h, attrs.as_ref()) {
                return Ok(vec![InlineNode::Math(math)]);
            }
        }
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
            // `<s>` and `<strike>` genuinely ARE strike - `~x~` is what Carve
            // spells them with - so they stay here. `<del>` does not: it has an
            // exact node of its own, one arm down (carve-rs#1223).
            "s" | "strike" => emphasis(EmphasisKind::Strike),
            // THE CRITICMARKUP PAIR, each on its own node. `{+ +}` renders back
            // to `<ins>` and `{- -}` to `<del>`, so neither element loses
            // anything on the way in or the way out.
            //
            // `<ins>` had no branch at all once and fell through to the
            // unwrapping path, losing its element AND being reported as
            // unsupported markup - twice wrong, since Carve can spell it.
            // `<del>` had the opposite failure: it sat in the strike arm above,
            // so it came back as `~x~` and RE-RENDERED AS `<s>`. The element
            // changed, which makes it the one shape in that neighborhood that
            // is not HTML-lossless, and the two halves of the same pair
            // disagreed. carve-js maps `del` to its `delete` node and carve-php
            // spells it `{- -}`, so this is also what the other two engines do.
            "ins" => InlineNode::CriticInsert(CriticInsert {
                attrs,
                children,
                pos: None,
            }),
            "del" => InlineNode::CriticDelete(CriticDelete {
                attrs,
                children,
                pos: None,
            }),
            "u" => emphasis(EmphasisKind::Underline),
            "mark" => emphasis(EmphasisKind::Highlight),
            "sub" => emphasis(EmphasisKind::Sub),
            "sup" => emphasis(EmphasisKind::Super),
            "code" => InlineNode::code(Self::text(h), attrs),
            // A DESTINATION CARVE CANNOT CARRY IS NOT A DESTINATION
            // (docs/html-import.md, markup-carve/carve#1601). Carve spells a
            // link's destination and an image's source in the same slot and has
            // NO spelling for an empty one: `[t]()` and `![t]()` are literal
            // text. So writing the empty slot does not write a link, it writes
            // four punctuation characters the HTML never held into the middle
            // of the prose.
            //
            // The rule is over the DESTINATION, not over the reason it is
            // missing: no attribute at all and a present-but-empty one are one
            // shape. What is written instead is what the element's content and
            // its SURVIVING attributes would produce without it - the span
            // where an attribute survives, the bare content where none does,
            // which is the attribute-less `<div>` boundary one layer down.
            //
            // THE DESTINATION IS NEVER REBUILT. `href=""` is what PART 9 §25's
            // URL sink denylist EMITS when it blanks a dangerous scheme while
            // keeping the visible text, so this is the importer reading Carve's
            // own hardened output. There is nothing in that HTML to rebuild the
            // destination from, and reconstructing one from a `title` or from
            // the anchor's text would undo the hardening.
            "a" if names_no_destination(Self::attr(h, "href").as_deref()) => {
                self.diag(
                    HtmlImportDiagnosticCode::ElementUnwrapped,
                    "Unwrapped <a> with no destination".into(),
                    HtmlImportSeverity::Info,
                    path,
                    h,
                );
                return Ok(unwrapped_content(attrs, children));
            }
            // AN IMAGE'S CONTENT IS ITS ALTERNATIVE TEXT: that is what every
            // target with no image shows for it, and what a browser shows for
            // one it cannot load, so it is the text this document's reader was
            // going to see either way.
            "img" if names_no_destination(Self::attr(h, "src").as_deref()) => {
                self.diag(
                    HtmlImportDiagnosticCode::ElementUnwrapped,
                    "Unwrapped <img> with no source".into(),
                    HtmlImportSeverity::Info,
                    path,
                    h,
                );
                let alt = Self::attr(h, "alt").unwrap_or_default();
                let content = if alt.is_empty() {
                    Vec::new()
                } else {
                    vec![InlineNode::text(alt)]
                };
                return Ok(unwrapped_content(attrs, content));
            }
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
            "br" => {
                // A hard break has NO ATTRIBUTE SLOT: `Break` carries only a
                // position, so `<br clear="all">` has nowhere to put it. Naming
                // the drop is all this can do; staying quiet would be the
                // silent loss carve-rs#1060 is about.
                self.report_unplaceable_attrs(
                    h,
                    attrs,
                    "br",
                    "a hard break carries no attributes",
                    path,
                );
                InlineNode::hard_break()
            }
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
                    h,
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
                    h,
                );
                // The inline twin of the block unwrap above: `<small>`,
                // `<bdi dir="rtl">`, `<bdo>`, `<ruby>`, `<button>`, `<label>`
                // keep their children and nothing else.
                self.report_unplaceable_attrs(
                    h,
                    attrs,
                    tag.as_str(),
                    "the element was unwrapped and has no node to carry it",
                    path,
                );
                return Ok(children);
            }
        };
        Ok(vec![node])
    }

    // ------------------------------------------------------------------
    // Adapter footnotes: word-processor footnote-shaped HTML to footnotes.
    //
    // Ports markup-carve/carve-php#1303 (with the branch pins of #1307) and
    // markup-carve/carve-js#1103. The shapes were measured, not recalled -
    // Word's two saves, Google Docs, LibreOffice and Pandoc 1.x agree on
    // almost nothing, and what all of them do have is a MUTUALLY LINKED
    // ANCHOR PAIR: the body reference addresses the note and the note
    // addresses the reference back. That pair, not a vendor class name and
    // not the `fn1`/`fnref1` id convention, is the signature matched here.
    //
    // The spec permits this shape of work - "Adapters may normalize
    // editor-specific markup before the core policy" - but it does not rule
    // on footnote import, so every decision below is this importer's,
    // written down rather than left silent. No diagnostics on the edge
    // cases, deliberately: in each of them the Carve source keeps what the
    // HTML said, so there is nothing lossy to report.
    // ------------------------------------------------------------------

    /// Recognize footnote pairs, record each reference site for `inline` to
    /// read as a footnote reference, detach the note blocks, and return their
    /// bodies keyed 1..N by document order.
    ///
    /// Labels are assigned 1..N over the notes in document order rather than
    /// parsed out of the ids: an id is generated navigation an engine
    /// regenerates, and `_ftn1` or `sdfootnote1sym` is not a label any Carve
    /// source could carry anyway.
    ///
    /// `heuristic` is the word-processor adapters' licence: with it a mutual
    /// anchor pair alone binds. Without it - `generic` and the editor adapters -
    /// only an anchor the producer MARKED with `role="doc-noteref"` opens a
    /// pair, which is authored DPUB-ARIA semantics rather than a guess, so a
    /// role-less document imports exactly as it did before. That is what lets
    /// Pandoc 2.11+ output import its footnotes without naming an adapter, and
    /// it is what the shared fixtures are written over (carve-rs#1313).
    fn adapter_footnotes(
        &mut self,
        root: &Handle,
        heuristic: bool,
    ) -> Result<BTreeMap<String, Vec<BlockNode>>, HtmlImportError> {
        let elements = footnote_document_elements(root);
        let mut order: HashMap<usize, usize> = HashMap::with_capacity(elements.len());
        for (index, element) in elements.iter().enumerate() {
            order.insert(node_key(element), index);
        }

        let targets = footnote_fragment_targets(&elements);
        let candidates = resolve_footnote_pair_direction(
            footnote_pair_candidates(&elements, &targets, heuristic),
            &order,
        );
        if candidates.is_empty() {
            return Ok(BTreeMap::new());
        }

        let definitions = attach_remaining_footnote_references(
            &elements,
            group_footnote_definitions(candidates, &order),
            heuristic,
        );

        let mut defs = BTreeMap::new();
        let mut containers: Vec<Handle> = Vec::new();
        let mut seen: HashSet<usize> = HashSet::new();

        for (index, definition) in definitions.iter().enumerate() {
            let label = (index + 1).to_string();
            if index == 0 {
                remove_footnote_separator(&definition.block);
            }

            let identities: Vec<String> = definition
                .refs
                .iter()
                .map(footnote_anchor_identity)
                .filter(|identity| !identity.is_empty())
                .collect();
            strip_footnote_backlinks(&definition.block, &identities, &definition.fragments);

            let body: Vec<Handle> = definition.block.children.borrow().clone();
            let blocks = self.blocks(&body, &format!("footnote[{label}]"), 1)?;
            defs.insert(label.clone(), blocks);

            for reference in &definition.refs {
                let site = footnote_reference_site(reference);
                self.footnote_refs
                    .insert(node_key(&site), (site.clone(), label.clone()));
            }

            if let Some(container) = parent_handle(&definition.block) {
                if seen.insert(node_key(&container)) {
                    containers.push(container);
                }
            }
            footnote_detach(&definition.block);
        }

        // Kept unique, because every note in one list names the SAME
        // container: pruning it once per note walked that list's children
        // once per note, which is quadratic on a document that is mostly
        // notes.
        // ONE MARKER FOR THE DOCUMENT, at the FIRST section that leaves a slot
        // with content after it. Every note in one list names the same
        // container, and a document with two endnotes sections has one place the
        // renderer will rebuild them, so a second directive would spell a
        // position no render can honour.
        let mut marked = false;
        for container in &containers {
            let removed_from = prune_empty_footnote_container(container);
            if !marked {
                marked = mark_footnote_placement(removed_from);
            }
        }

        Ok(defs)
    }
}

/// The nodes an import walks, and the ones a diagnostic `path` is rooted at.
///
/// A `path` names the imported FRAGMENT, so the wrappers an HTML parser is
/// obliged to synthesize are not part of it. `<html>`, `<head>` and `<body>`
/// contribute neither a path segment nor a sibling position: their children
/// stand exactly where they stood, in one run. html5ever's `parse_document`
/// builds all three for any input, a bare `<p>` included, so reading the
/// document's own children made every path lead with `/html[1]/body[2]` and
/// name the parser instead of the input.
///
/// A doctype is skipped rather than kept as a sibling. It is not a node of the
/// fragment, and counting it would shift the index of everything after it - the
/// same reason the wrappers do not count.
///
/// A comment is kept: it IS a node of the fragment, and drops out of the
/// content on its own further down without disturbing the numbering.
fn fragment_top_level(node: &Handle) -> Vec<Handle> {
    let mut out = Vec::new();
    for child in node.children.borrow().iter() {
        match &child.data {
            NodeData::Doctype { .. } => {}
            NodeData::Element { name, .. }
                if matches!(name.local.as_ref(), "html" | "head" | "body") =>
            {
                out.extend(fragment_top_level(child));
            }
            _ => out.push(child.clone()),
        }
    }
    out
}

/// Every element in the subtree, in document order.
fn footnote_document_elements(root: &Handle) -> Vec<Handle> {
    let mut elements = Vec::new();
    let mut stack: Vec<Handle> = root.children.borrow().iter().rev().cloned().collect();
    while let Some(node) = stack.pop() {
        if !matches!(node.data, NodeData::Element { .. }) {
            continue;
        }
        let children = node.children.borrow();
        for child in children.iter().rev() {
            stack.push(child.clone());
        }
        drop(children);
        elements.push(node);
    }
    elements
}

/// Map every same-document fragment name to the element it addresses.
///
/// `id` first and `name` second, in two passes rather than one, so an `id`
/// always wins over the legacy `<a name>` form when both spell one fragment.
fn footnote_fragment_targets(elements: &[Handle]) -> HashMap<String, Handle> {
    let mut targets: HashMap<String, Handle> = HashMap::new();
    for element in elements {
        if let Some(id) = Importer::attr(element, "id") {
            if !id.is_empty() {
                targets.entry(id).or_insert_with(|| element.clone());
            }
        }
    }
    for element in elements {
        if Importer::tag(element).as_deref() != Some("a") {
            continue;
        }
        if let Some(name) = Importer::attr(element, "name") {
            if !name.is_empty() {
                targets.entry(name).or_insert_with(|| element.clone());
            }
        }
    }
    targets
}

/// Every anchor that could be a footnote reference, with the block it would
/// bind to. A candidate needs the mutual back-link or an explicit reference
/// marker; an anchor inside its own would-be note is never one.
fn footnote_pair_candidates(
    elements: &[Handle],
    targets: &HashMap<String, Handle>,
    heuristic: bool,
) -> Vec<FootnoteCandidate> {
    let mut anchors: Vec<(Handle, String)> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    for element in elements {
        if Importer::tag(element).as_deref() != Some("a") {
            continue;
        }
        let href = Importer::attr(element, "href").unwrap_or_default();
        let Some(fragment) = href.strip_prefix('#') else {
            continue;
        };
        if fragment.is_empty() || !targets.contains_key(fragment) {
            continue;
        }
        used.insert(fragment.to_string());
        anchors.push((element.clone(), fragment.to_string()));
    }

    let mut candidates = Vec::new();
    for (anchor, fragment) in anchors {
        // OUTSIDE THE HEURISTIC ONLY THE AUTHORED ROLE OPENS A PAIR. The vendor
        // classes belong to the heuristic: a class is a styling hook, where the
        // role is a statement about what the element IS.
        if !heuristic && Importer::attr(&anchor, "role").as_deref() != Some("doc-noteref") {
            continue;
        }
        let Some(block) = resolve_footnote_definition_block(&targets[&fragment], &used) else {
            continue;
        };
        if footnote_contains(&block, &anchor) {
            continue;
        }

        let identity = footnote_anchor_identity(&anchor);
        let mutual = !identity.is_empty() && footnote_block_links_to(&block, &identity);
        if !mutual && !is_footnote_reference_marked(&anchor) {
            continue;
        }

        candidates.push(FootnoteCandidate {
            reference: anchor,
            block,
            fragment,
        });
    }
    candidates
}

/// The block a reference's target belongs to.
///
/// The target itself when it is already a block (Pandoc's `<li id="fn1">`),
/// otherwise the nearest block ancestor of the anchor the fragment names.
/// Then ONE guarded climb, because Word and LibreOffice wrap each note in a
/// dedicated `<div id=...>` and the body can be several paragraphs inside it:
/// the climb only happens into a wrapper that carries an id and holds exactly
/// one referenced target, which is what keeps a shared container (Google Docs'
/// one trailing `<div>` around every note) from swallowing its siblings.
///
/// A fragment whose nearest block would be the document itself is refused -
/// taking it would move every block in the document into one note - which here
/// is the climb running off the top past `<body>` and `<html>`, neither of
/// which is a definition block.
fn resolve_footnote_definition_block(target: &Handle, used: &HashSet<String>) -> Option<Handle> {
    let mut block = target.clone();
    while !Importer::tag(&block)
        .map(|tag| FOOTNOTE_DEFINITION_BLOCKS.contains(&tag.as_str()))
        .unwrap_or(false)
    {
        let parent = parent_handle(&block)?;
        if !matches!(parent.data, NodeData::Element { .. }) {
            return None;
        }
        block = parent;
    }

    if let Some(parent) = parent_handle(&block) {
        let wraps = Importer::tag(&parent)
            .map(|tag| FOOTNOTE_WRAPPER_BLOCKS.contains(&tag.as_str()))
            .unwrap_or(false);
        if wraps
            && !Importer::attr(&parent, "id").unwrap_or_default().is_empty()
            && count_footnote_targets(&parent, used) == 1
        {
            block = parent;
        }
    }

    Some(block)
}

/// How many referenced fragment targets this element holds, itself included.
fn count_footnote_targets(node: &Handle, used: &HashSet<String>) -> usize {
    let mut count = usize::from(is_footnote_fragment_target(node, used));
    for child in node.children.borrow().iter() {
        if matches!(child.data, NodeData::Element { .. }) {
            count += count_footnote_targets(child, used);
        }
    }
    count
}

fn is_footnote_fragment_target(node: &Handle, used: &HashSet<String>) -> bool {
    if let Some(id) = Importer::attr(node, "id") {
        if !id.is_empty() && used.contains(&id) {
            return true;
        }
    }
    if Importer::tag(node).as_deref() != Some("a") {
        return false;
    }
    Importer::attr(node, "name")
        .map(|name| !name.is_empty() && used.contains(&name))
        .unwrap_or(false)
}

/// Keep one side of every mutually linked anchor pair.
///
/// The pair is symmetric, so both directions produce a candidate and one of
/// them is the back-link reading as a reference. An explicit marker decides
/// where there is one; otherwise document order does, because a footnote
/// reference precedes the note it opens in every export shape measured.
fn resolve_footnote_pair_direction(
    candidates: Vec<FootnoteCandidate>,
    order: &HashMap<usize, usize>,
) -> Vec<FootnoteCandidate> {
    let mut by_reference: HashMap<usize, usize> = HashMap::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        by_reference.insert(node_key(&candidate.reference), index);
    }

    let mut kept = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let inverse = inverse_footnote_candidate(&candidates, &by_reference, candidate);
        if inverse
            .map(|other| footnote_reference_side_wins(&candidates[other], candidate, order))
            .unwrap_or(false)
        {
            continue;
        }
        kept.push(index);
    }

    let mut out = Vec::with_capacity(kept.len());
    let mut remaining: Vec<Option<FootnoteCandidate>> = candidates.into_iter().map(Some).collect();
    for index in kept {
        if let Some(candidate) = remaining[index].take() {
            out.push(candidate);
        }
    }
    out
}

/// The candidate that reads the same mutual pair from the other end.
///
/// Found through the back anchor the candidate's own block holds rather than
/// by comparing every candidate with every other: a document with a thousand
/// notes made that scan a thousand times a thousand containment walks, and the
/// anchor names the inverse directly.
fn inverse_footnote_candidate(
    candidates: &[FootnoteCandidate],
    by_reference: &HashMap<usize, usize>,
    candidate: &FootnoteCandidate,
) -> Option<usize> {
    let identity = footnote_anchor_identity(&candidate.reference);
    if identity.is_empty() {
        return None;
    }
    let wanted = format!("#{identity}");
    for anchor in footnote_anchors_under(&candidate.block) {
        if Importer::attr(&anchor, "href").as_deref() != Some(wanted.as_str()) {
            continue;
        }
        let Some(&index) = by_reference.get(&node_key(&anchor)) else {
            continue;
        };
        if footnote_contains(&candidates[index].block, &candidate.reference) {
            return Some(index);
        }
    }
    None
}

fn footnote_reference_side_wins(
    first: &FootnoteCandidate,
    second: &FootnoteCandidate,
    order: &HashMap<usize, usize>,
) -> bool {
    let first_marked = is_footnote_reference_marked(&first.reference);
    let second_marked = is_footnote_reference_marked(&second.reference);
    if first_marked != second_marked {
        return first_marked;
    }

    let first_back = is_footnote_backlink_marked(&first.reference);
    let second_back = is_footnote_backlink_marked(&second.reference);
    if first_back != second_back {
        return second_back;
    }

    order.get(&node_key(&first.reference)).copied().unwrap_or(0)
        < order
            .get(&node_key(&second.reference))
            .copied()
            .unwrap_or(0)
}

/// One entry per definition block, carrying every reference bound to it.
///
/// A block that contains another definition block is a container, not a note:
/// keeping both would move a subtree into two places at once. The containers
/// are found by climbing from each block, one walk per note rather than one
/// per PAIR of notes.
fn group_footnote_definitions(
    candidates: Vec<FootnoteCandidate>,
    order: &HashMap<usize, usize>,
) -> Vec<FootnoteGroup> {
    let mut index_of: HashMap<usize, usize> = HashMap::new();
    let mut groups: Vec<FootnoteGroup> = Vec::new();
    for candidate in candidates {
        let key = node_key(&candidate.block);
        let index = *index_of.entry(key).or_insert_with(|| {
            groups.push(FootnoteGroup {
                block: candidate.block.clone(),
                refs: Vec::new(),
                fragments: Vec::new(),
            });
            groups.len() - 1
        });
        groups[index].refs.push(candidate.reference);
        if !groups[index].fragments.contains(&candidate.fragment) {
            groups[index].fragments.push(candidate.fragment);
        }
    }

    let mut containers: HashSet<usize> = HashSet::new();
    for group in &groups {
        let mut ancestor = parent_handle(&group.block);
        while let Some(node) = ancestor {
            if index_of.contains_key(&node_key(&node)) {
                containers.insert(node_key(&node));
            }
            ancestor = parent_handle(&node);
        }
    }

    let mut kept: Vec<FootnoteGroup> = groups
        .into_iter()
        .filter(|group| !containers.contains(&node_key(&group.block)))
        .collect();
    kept.sort_by_key(|group| order.get(&node_key(&group.block)).copied().unwrap_or(0));
    kept
}

/// Bind every remaining anchor that addresses a confirmed note.
///
/// Once a block IS a footnote definition, an anchor pointing at it is a
/// reference to it whatever it looks like. This matters for the second and
/// later reference to one note: only one of them can be the back-link's
/// target, so the mutual pair that confirmed the note cannot confirm them, and
/// without this they stayed literal links beside a `[^1]`. An anchor inside a
/// note stays a link - a note's body may address another note.
fn attach_remaining_footnote_references(
    elements: &[Handle],
    mut definitions: Vec<FootnoteGroup>,
    heuristic: bool,
) -> Vec<FootnoteGroup> {
    let mut by_fragment: HashMap<String, usize> = HashMap::new();
    for (index, definition) in definitions.iter().enumerate() {
        for fragment in &definition.fragments {
            by_fragment.insert(fragment.clone(), index);
        }
    }

    // Which elements sit inside a note, computed once: asking each anchor
    // whether it is inside any note walked the tree once per anchor and per
    // note, which is quadratic on a document that is mostly notes.
    let mut inside: HashSet<usize> = HashSet::new();
    for definition in &definitions {
        for element in footnote_document_elements(&definition.block) {
            inside.insert(node_key(&element));
        }
        inside.insert(node_key(&definition.block));
    }

    for element in elements {
        if Importer::tag(element).as_deref() != Some("a") {
            continue;
        }
        // Outside the heuristic an unmarked anchor addressing a note is a LINK,
        // not a reference: the role is the whole signal, so a content link to
        // `#fn1` in a role-marked document keeps the author's shape. The marked
        // candidates already sit in their groups; this loop exists for the
        // unmarked SECOND reference the heuristic binds.
        if !heuristic && Importer::attr(element, "role").as_deref() != Some("doc-noteref") {
            continue;
        }
        let href = Importer::attr(element, "href").unwrap_or_default();
        let Some(fragment) = href.strip_prefix('#') else {
            continue;
        };
        let Some(&index) = by_fragment.get(fragment) else {
            continue;
        };
        if inside.contains(&node_key(element)) {
            continue;
        }
        if !definitions[index]
            .refs
            .iter()
            .any(|reference| Rc::ptr_eq(reference, element))
        {
            definitions[index].refs.push(element.clone());
        }
    }

    definitions
}

fn footnote_anchor_identity(anchor: &Handle) -> String {
    match Importer::attr(anchor, "id") {
        Some(id) if !id.is_empty() => id,
        _ => Importer::attr(anchor, "name").unwrap_or_default(),
    }
}

fn footnote_block_links_to(block: &Handle, fragment: &str) -> bool {
    let wanted = format!("#{fragment}");
    footnote_anchors_under(block)
        .iter()
        .any(|anchor| Importer::attr(anchor, "href").as_deref() == Some(wanted.as_str()))
}

fn footnote_anchors_under(node: &Handle) -> Vec<Handle> {
    footnote_document_elements(node)
        .into_iter()
        .filter(|element| Importer::tag(element).as_deref() == Some("a"))
        .collect()
}

/// `footnoteRef` is Pandoc 1.x's spelling of `footnote-ref`, which it used
/// together with a back-link carrying no attributes at all.
fn is_footnote_reference_marked(anchor: &Handle) -> bool {
    if Importer::attr(anchor, "role").as_deref() == Some("doc-noteref") {
        return true;
    }
    footnote_has_class(anchor, "footnote-ref") || footnote_has_class(anchor, "footnoteRef")
}

fn is_footnote_backlink_marked(anchor: &Handle) -> bool {
    if Importer::attr(anchor, "role").as_deref() == Some("doc-backlink") {
        return true;
    }
    footnote_has_class(anchor, "footnote-back")
}

fn footnote_has_class(node: &Handle, wanted: &str) -> bool {
    Importer::attr(node, "class")
        .unwrap_or_default()
        .split_whitespace()
        .any(|class| class == wanted)
}

fn node_key(handle: &Handle) -> usize {
    Rc::as_ptr(handle) as usize
}

/// Whether an element carries `wanted` among its space-separated classes.
fn has_class(node: &Handle, wanted: &str) -> bool {
    Importer::attr(node, "class")
        .unwrap_or_default()
        .split_whitespace()
        .any(|class| class == wanted)
}

/// Whether this `<p class="admonition-title">` is one the renderer's own
/// counter counted.
///
/// WHICH PARAGRAPHS THE COUNTER COUNTS is the renderer's condition, not the
/// class alone. It increments for a CANONICAL admonition with a title and no
/// authored name, and that is exactly when it emits the id and points the
/// `<aside>`'s `aria-labelledby` at it - so a paragraph qualifies when its
/// parent aside names it back. Counting every `admonition-title` instead would
/// desync on the two shapes that carry the class and no counter: a
/// non-canonical `::: custom "T"` (a `<div>`, title with no id) and a canonical
/// one the author named (`aria-label` wins, title with no id).
/// Whether this is the paragraph a container's TITLE renders as.
///
/// The class alone, because the class alone is what the renderer always writes.
/// The generated `id` beside it is conditional - `render_admonition` emits one
/// only for a Tier-1 kind with no authored name - so a `::: sidebar "A"` (a
/// `<div>`, no id) and a `::: note "A"` carrying an authored `aria-label` (no id
/// either) both render a bare `<p class="admonition-title">`. Reading the id as
/// the marker left the title of both in the body, where it was written back as
/// an ordinary paragraph carrying the renderer's class.
///
/// [`is_counted_admonition_title`] is the NARROWER question and stays narrow: it
/// asks whether the renderer's counter produced this id, which is a question
/// about dropping a derived value, not about what the paragraph IS.
/// A value lifted off the front of a container's body onto its OPENER, plus
/// what is left of the body and that body's own paths.
///
/// Two lifts have this shape - the quoted title and the grouping `[label]` -
/// and naming it keeps their signatures readable and stops `type_complexity`
/// firing on each of them separately.
type Lifted<T> = Result<(Option<T>, Vec<Handle>, Vec<String>), HtmlImportError>;

fn is_div_label(node: &Handle) -> bool {
    Importer::tag(node).as_deref() == Some("p") && has_class(node, "div-label")
}

fn is_admonition_title(node: &Handle) -> bool {
    Importer::tag(node).as_deref() == Some("p") && has_class(node, "admonition-title")
}

/// The base every derived admonition title id starts with, and therefore the
/// only prefix an id has to carry to be able to collide with one.
const ADMONITION_ID_PREFIX: &str = "adm-";

fn is_counted_admonition_title(node: &Handle) -> bool {
    if !is_admonition_title(node) {
        return false;
    }
    let Some(id) = Importer::attr(node, "id") else {
        return false;
    };
    let Some(parent) = parent_handle(node) else {
        return false;
    };
    Importer::tag(&parent).as_deref() == Some("aside")
        && has_class(&parent, "admonition")
        && Importer::attr(&parent, "aria-labelledby").as_deref() == Some(id.as_str())
}

/// The id the renderer derives for this title paragraph - `adm-1`, `adm-2`, …
/// in document order, which is the order the renderer's own counter runs in,
/// PUT THROUGH THE ID NAMESPACE the renderer allocates in.
///
/// Counted from the document root rather than upward from the node, so the
/// ordinal is the renderer's and not this subtree's. The walk is ITERATIVE: the
/// importer's depth limit is a counter a caller may raise past what the stack
/// holds, and a recursive prewalk would overflow before the counter spoke.
///
/// THE COUNTER IS NOT THE ID (carve-rs#1258). `render_admonition` does not write
/// `adm-{N}`; it writes `document_ids::unique_id("adm-{N}")`, and that registry
/// is seeded with every id the document already carries, so a name the author
/// took first pushes the generated one to the next free numeric suffix. Against
/// a document whose author wrote `{#adm-1}`, the renderer emits `adm-1-2` and
/// predicting the bare counter looked for `adm-1`, missed, and reported a drop
/// that had not happened - the mirror of markup-carve/carve-php#1579, which was
/// accepted for removing three rows of exactly that kind.
///
/// So the allocation is modelled rather than the counter, and the match stays an
/// EQUALITY match on the value the renderer would actually write. Matching the
/// SHAPE `adm-N` instead is the guess this rule deliberately does not make, and
/// `a_counter_shaped_id_on_a_title_no_counter_counted_is_kept` pins that.
fn admonition_title_id(node: &Handle) -> Option<String> {
    let mut root = node.clone();
    while let Some(parent) = parent_handle(&root) {
        root = parent;
    }
    // PASS A: the namespace the renderer's registry holds when the first
    // admonition renders - every explicit `{#id}` and every heading id, which
    // in rendered HTML are simply the `id` attributes on the page. The counted
    // title paragraphs are SKIPPED: their ids are the ones being predicted, and
    // seeding the registry with them would make every prediction collide with
    // the value it is trying to reproduce.
    let mut used: BTreeMap<String, usize> = BTreeMap::new();
    let mut titles: Vec<Handle> = Vec::new();
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        if is_counted_admonition_title(&current) {
            titles.push(current.clone());
        } else if let Some(id) =
            Importer::attr(&current, "id").filter(|id| id.starts_with(ADMONITION_ID_PREFIX))
        {
            // ONLY THE IDS THAT CAN COLLIDE. Every name the allocation below
            // looks up starts with `adm-`, so filtering to that prefix is
            // EXACT rather than a heuristic - and it keeps the map the size of
            // the colliding names instead of the size of the document, which
            // matters because this walk already runs once per title.
            //
            // First reservation wins, exactly as `DocumentIdRegistry::reserve`
            // has it; a repeat is a no-op rather than a second claim.
            used.entry(id).or_insert(1);
        }
        // Reversed, so the children are visited left to right once popped -
        // the order the renderer emitted them in.
        for child in current.children.borrow().iter().rev() {
            stack.push(child.clone());
        }
    }
    // PASS B: allocate in document order, so a title's id depends on what the
    // titles before it took as well as on what the author took.
    for (index, title) in titles.iter().enumerate() {
        let allocated =
            allocate_document_id(&mut used, &format!("{ADMONITION_ID_PREFIX}{}", index + 1));
        if Rc::ptr_eq(title, node) {
            return Some(allocated);
        }
    }
    None
}

/// `DocumentIdRegistry::unique_id`, replayed over an id set read off the HTML.
///
/// A second copy of nine lines, and deliberately not a call into the renderer's
/// registry: that one is a thread-local installed for the duration of one HTML
/// RENDER, seeded from a `Document` this side does not have. What the importer
/// has is the rendered page, where the same namespace is spelled as `id`
/// attributes. The rule it replays is `base`, then `base-2`, `base-3`, … past
/// anything already taken, remembering the per-base counter so repeated calls
/// continue rather than restart.
fn allocate_document_id(used: &mut BTreeMap<String, usize>, base: &str) -> String {
    let Some(&count) = used.get(base) else {
        used.insert(base.to_owned(), 1);
        return base.to_owned();
    };
    let mut n = count;
    let candidate = loop {
        n += 1;
        let candidate = format!("{base}-{n}");
        if !used.contains_key(&candidate) {
            break candidate;
        }
    };
    used.insert(base.to_owned(), n);
    used.insert(candidate.clone(), 1);
    candidate
}

/// The node's parent, restored into the cell it was read out of.
///
/// `Cell` has no borrow, so the only way to look at a `Weak` inside one is to
/// take it and put it back.
fn parent_handle(node: &Handle) -> Option<Handle> {
    let weak = node.parent.take();
    let parent = weak.as_ref().and_then(|parent| parent.upgrade());
    node.parent.set(weak);
    parent
}

fn footnote_contains(ancestor: &Handle, node: &Handle) -> bool {
    let mut current = parent_handle(node);
    while let Some(handle) = current {
        if Rc::ptr_eq(&handle, ancestor) {
            return true;
        }
        current = parent_handle(&handle);
    }
    false
}

fn footnote_detach(node: &Handle) {
    let Some(parent) = parent_handle(node) else {
        return;
    };
    let mut children = parent.children.borrow_mut();
    if let Some(index) = children.iter().position(|child| Rc::ptr_eq(child, node)) {
        children.remove(index);
    }
    drop(children);
    node.parent.set(None);
}

fn footnote_previous_sibling(node: &Handle) -> Option<Handle> {
    let parent = parent_handle(node)?;
    let children = parent.children.borrow();
    let index = children.iter().position(|child| Rc::ptr_eq(child, node))?;
    if index == 0 {
        return None;
    }
    Some(children[index - 1].clone())
}

/// Remove the rule that separates the notes from the body.
///
/// Every producer measured emits one, and it is chrome rather than content:
/// Pandoc puts `<hr />` inside the section, Word `<br clear=all><hr ...>`
/// inside the footnote-list div, Google Docs a bare `<hr class="cN">` as a
/// sibling of the notes. Only the first two would be swept up by pruning an
/// emptied container, so the separator is looked for explicitly - at the first
/// note, and at each of its ancestors, taking only what immediately precedes
/// it.
fn remove_footnote_separator(first: &Handle) {
    let mut node = first.clone();
    loop {
        let mut previous = footnote_previous_sibling(&node);
        while let Some(candidate) = &previous {
            if !is_footnote_chrome_node(candidate) {
                break;
            }
            previous = footnote_previous_sibling(candidate);
        }

        if let Some(candidate) = previous {
            match Importer::tag(&candidate).as_deref() {
                Some("hr") | Some("br") => {
                    footnote_detach(&candidate);
                    continue;
                }
                _ => return,
            }
        }

        let Some(parent) = parent_handle(&node) else {
            return;
        };
        match Importer::tag(&parent).as_deref() {
            Some("body") | Some("html") | None => return,
            Some(_) => node = parent,
        }
    }
}

/// Whether a node is part of the separator's packaging rather than content.
///
/// Word brackets the `<br clear=all><hr>` inside the footnote-list div in
/// downlevel-revealed conditionals, `<![if !supportFootnotes]>` and the
/// matching `<![endif]>`. Those are NOT comments in the source, but html5ever
/// follows the HTML grammar and reads `<!` without `--` as a BOGUS COMMENT, so
/// here they arrive as comment nodes - measured, not assumed - and the comment
/// branch is what recognizes them. carve-php reads the same bytes back as TEXT
/// because libxml has no such production, which is why its port carries a
/// pattern for the text spelling that this one has no way to reach.
fn is_footnote_chrome_node(node: &Handle) -> bool {
    match &node.data {
        NodeData::Comment { .. } => true,
        NodeData::Text { contents } => contents.borrow().trim().is_empty(),
        _ => false,
    }
}

/// Remove the navigation an engine regenerates: the back-link, and the marker
/// anchor Word, Google Docs and LibreOffice put it on.
///
/// Carried into the note body it would render as a stray link to a fragment
/// that no longer exists, and the visible marker it wraps (`[1]`, `1`, the
/// return arrow) would be written into the note's own text. The third clause -
/// an anchor that IS the fragment target the reference points at, with a
/// fragment href - is what removes the marker anchor that is the note's anchor
/// and its back-link and its visible marker in one element.
fn strip_footnote_backlinks(block: &Handle, identities: &[String], fragments: &[String]) {
    for anchor in footnote_anchors_under(block) {
        let href = Importer::attr(&anchor, "href").unwrap_or_default();
        let target = href.strip_prefix('#');
        let points_back = target
            .map(|fragment| identities.iter().any(|identity| identity == fragment))
            .unwrap_or(false);
        let is_marker = target.is_some() && fragments.contains(&footnote_anchor_identity(&anchor));
        if !is_footnote_backlink_marked(&anchor) && !points_back && !is_marker {
            continue;
        }

        let parent = parent_handle(&anchor);
        footnote_detach(&anchor);
        let Some(parent) = parent else { continue };
        if !matches!(
            Importer::tag(&parent).as_deref(),
            Some("sup") | Some("span")
        ) {
            continue;
        }
        let emptied = parent
            .children
            .borrow()
            .iter()
            .all(|child| match &child.data {
                NodeData::Element { .. } => false,
                NodeData::Text { contents } => contents.borrow().trim().is_empty(),
                _ => true,
            });
        if emptied {
            footnote_detach(&parent);
        }
    }
}

/// The node a reference occupies: the anchor, or the `<sup>` that holds
/// nothing but the anchor.
///
/// Google Docs and Pandoc put the `<sup>` outside the anchor, so taking only
/// the anchor would leave `{^...^}` wrapped around the reference. One carrying
/// anything else - an element or non-blank text - keeps its content, and the
/// reference binds inside it.
fn footnote_reference_site(reference: &Handle) -> Handle {
    let Some(parent) = parent_handle(reference) else {
        return reference.clone();
    };
    if Importer::tag(&parent).as_deref() != Some("sup") {
        return reference.clone();
    }
    for child in parent.children.borrow().iter() {
        match &child.data {
            NodeData::Element { .. } if !Rc::ptr_eq(child, reference) => {
                return reference.clone();
            }
            NodeData::Text { contents } if !contents.borrow().trim().is_empty() => {
                return reference.clone();
            }
            _ => {}
        }
    }
    parent.clone()
}

/// Drop a container the notes left empty, so the `<hr>` and the `<ol>` that
/// held them do not import as a thematic break beside an empty list.
///
/// A separator written AFTER the notes survives the explicit search and is
/// swept up here instead.
fn prune_empty_footnote_container(node: &Handle) -> Option<(Handle, usize)> {
    let mut current = Some(node.clone());
    // The slot the OUTERMOST removed node sat in, which is the one the section
    // itself occupied. An inner one names a position inside a container that is
    // about to be detached too, so a marker put there would be detached with it.
    let mut removed_from = None;
    while let Some(handle) = current {
        match Importer::tag(&handle).as_deref() {
            None | Some("body") | Some("html") => return removed_from,
            Some(_) => {}
        }
        let holds_content = handle.children.borrow().iter().any(|child| {
            if is_footnote_chrome_node(child) {
                return false;
            }
            !matches!(Importer::tag(child).as_deref(), Some("hr") | Some("br"))
        });
        if holds_content {
            return removed_from;
        }
        let parent = parent_handle(&handle);
        let index = parent.as_ref().and_then(|parent| {
            parent
                .children
                .borrow()
                .iter()
                .position(|child| Rc::ptr_eq(child, &handle))
        });
        footnote_detach(&handle);
        if let (Some(parent), Some(index)) = (parent.clone(), index) {
            removed_from = Some((parent, index));
        }
        current = parent;
    }
    removed_from
}

/// The synthetic element `mark_footnote_placement` leaves where a non-final
/// endnotes section sat. It exists only between the footnote pass and the block
/// walk, and no real HTML input can carry the name.
const FOOTNOTE_PLACEMENT_TAG: &str = "carve-footnote-placement";

/// Put a `::: footnotes` directive back where the endnotes section stood.
///
/// ONLY WHEN SOMETHING ACTUALLY FOLLOWS IT, checked OUTWARD through the
/// ancestors rather than among the immediate siblings alone: a section last in a
/// `<div>` that is itself followed by a paragraph is still not last in the
/// document. A section that IS last needs no directive and gets none - the
/// definitions already render there, and writing one would put a construct in
/// the source the input never distinguished - so every document that was already
/// right stays byte-identical.
fn mark_footnote_placement(removed_from: Option<(Handle, usize)>) -> bool {
    let Some((parent, index)) = removed_from else {
        return false;
    };
    if !footnote_content_follows(&parent, index) {
        return false;
    }
    let marker = Node::new(NodeData::Element {
        name: QualName::new(None, ns!(html), FOOTNOTE_PLACEMENT_TAG.into()),
        attrs: RefCell::new(Vec::new()),
        template_contents: RefCell::new(None),
        mathml_annotation_xml_integration_point: false,
    });
    marker.parent.set(Some(Rc::downgrade(&parent)));
    let mut children = parent.children.borrow_mut();
    let at = index.min(children.len());
    children.insert(at, marker);
    true
}

/// Is there content after INDEX in PARENT, or after PARENT in any ancestor?
fn footnote_content_follows(parent: &Handle, index: usize) -> bool {
    let mut node = Some(parent.clone());
    let mut from = index;
    while let Some(handle) = node {
        if handle.children.borrow()[from.min(handle.children.borrow().len())..]
            .iter()
            .any(|child| !is_footnote_chrome_node(child))
        {
            return true;
        }
        let Some(up) = parent_handle(&handle) else {
            return false;
        };
        let Some(at) = up
            .children
            .borrow()
            .iter()
            .position(|child| Rc::ptr_eq(child, &handle))
        else {
            return false;
        };
        from = at + 1;
        node = Some(up);
    }
    false
}

/// Whether an HTML element name is one of the seven PART 9 §10 spells as a
/// compact span attribute.
///
/// Whether an `href` or a `src` names no destination the source can carry.
///
/// EMPTY IS A PROPERTY OF THE STRING, read the way an HTML URL attribute is
/// read: zero length, or zero length once leading and trailing ASCII whitespace
/// is stripped, because that is what a URL parser strips before resolving one.
/// A value that is merely unusual is not empty and is kept. `None` - the
/// attribute absent altogether - is the same shape as the empty one, since the
/// rule is over the destination rather than over the reason it is missing.
fn names_no_destination(value: Option<&str>) -> bool {
    match value {
        None => true,
        Some(v) => v.trim_matches(|c: char| c.is_ascii_whitespace()).is_empty(),
    }
}

/// What an unwrapped element leaves behind: the span where an attribute
/// survives, the bare content where none does.
///
/// The same boundary `<div>` takes one layer down, and the same question - what
/// is the element still needed to hold? An empty `Attrs` never reaches here:
/// `attrs` returns `None` for an element with nothing left to carry.
fn unwrapped_content(attrs: Option<Attrs>, children: Vec<InlineNode>) -> Vec<InlineNode> {
    match attrs {
        Some(attrs) => vec![InlineNode::Span(Span {
            attrs: Some(attrs),
            children,
            injected: false,
            pos: None,
        })],
        None => children,
    }
}

/// The list and the value mapping are the renderer's, read rather than
/// repeated: a name that joins or leaves the set, or starts carrying its value
/// somewhere else, cannot be right in the renderer and stale in the importer.
fn is_semantic_span_tag(tag: &str) -> bool {
    EXTENDED_SEMANTIC_SPAN_ORDER.contains(&tag)
}

/// The characters PART 11 §7 calls LAYOUT, and nothing else.
///
/// §7 draws the content-versus-layout line at PART 2's two-character
/// `whitespace` terminal (`whitespace = \' \' | \'\\t\'`, grammar.ebnf:4183),
/// "together with the line terminators an HTML parser folds into them". EVERY
/// other character is content, so NO-BREAK SPACE (U+00A0), NARROW NO-BREAK
/// SPACE (U+202F) and IDEOGRAPHIC SPACE (U+3000) are kept exactly as any letter
/// is - a lone content-space line parses back as a PARAGRAPH where a lone ASCII
/// space line is a BLANK LINE, which is the whole of why the two differ.
///
/// NOT `char::is_whitespace`, which is Unicode `White_Space` and holds all
/// three of those. Collapsing through it normalized a content space to an ASCII
/// one, which §7 forbids outright: it keeps a node while discarding the single
/// property that separates U+00A0 from a space, and the paragraph it leaves is
/// unspellable, so it vanishes when the writer runs and the two import exits
/// then disagree about a document the importer built itself
/// (markup-carve/carve-rs#1299, markup-carve/carve#1628).
///
/// The line terminators are here because an HTML parser folds them into the
/// same run: `<p>a\nb</p>` is one space between two words, not a break.
fn is_layout_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{000c}')
}

fn collapse(s: &str) -> String {
    let value = s
        .split(is_layout_space)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if value.is_empty() {
        return if s.is_empty() {
            String::new()
        } else {
            " ".into()
        };
    }
    format!(
        "{}{}{}",
        if s.starts_with(is_layout_space) {
            " "
        } else {
            ""
        },
        value,
        if s.ends_with(is_layout_space) {
            " "
        } else {
            ""
        }
    )
}

/// Whether an inline run holds text and nothing in it is content.
///
/// PART 11 §7's second row: a block element whose every character is LAYOUT
/// builds no node at all. Asked of the produced run rather than of the source
/// text, so `collapse` above has already decided which characters survived -
/// one reading of the line, not two.
///
/// An EMPTY run is not this shape. `<p></p>` holds no character to classify,
/// nothing was dropped, and PART 11 §10j names the empty paragraph as the
/// sibling shape whose treatment already keeps §1 - so it is left exactly as it
/// was.
fn is_layout_only(nodes: &[InlineNode]) -> bool {
    !nodes.is_empty()
        && nodes.iter().all(|n| match n {
            InlineNode::Text(t) => !t.value.is_empty() && t.value.chars().all(is_layout_space),
            _ => false,
        })
}

/// The edge whitespace of a SYNTHESIZED paragraph, removed.
///
/// The paragraph `blocks_at` wraps a bare inline run in is not an element the
/// document contains - it is invented to give the run somewhere to live - so
/// the whitespace at its two ends is not content, it is the inter-element
/// formatting whitespace that separated the run from the block markup beside
/// it. No target renders that whitespace, and Carve cannot hold it: a leading
/// space on a paragraph line is INDENTATION, which is syntax.
///
/// Only the two ends are touched, and only whitespace. Whitespace INSIDE the
/// run separates words and stays exactly as it is.
fn trim_edge_whitespace(mut nodes: Vec<InlineNode>) -> Vec<InlineNode> {
    while let Some(InlineNode::Text(first)) = nodes.first_mut() {
        first.value = first.value.trim_start().to_string();
        if first.value.is_empty() {
            nodes.remove(0);
        } else {
            break;
        }
    }
    while let Some(InlineNode::Text(last)) = nodes.last_mut() {
        last.value = last.value.trim_end().to_string();
        if last.value.is_empty() {
            nodes.pop();
        } else {
            break;
        }
    }
    nodes
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
        document_order: HashMap::new(),
        nodes: 0,
        quote_depth: 0,
        unspellable: Vec::new(),
        footnote_refs: HashMap::new(),
    };
    // BEFORE the adapter pass, which rewrites footnote-shaped HTML and detaches
    // what it consumes: the numbers have to be on the tree as the AUTHOR wrote
    // it, or a definition's diagnostics would sort by where the rewrite left it
    // rather than by where it stands in the document.
    importer.number_document_order(&dom.document);
    // Rewrite an editor's footnote-shaped HTML before the core policy reads
    // the tree, exactly as the adapter contract allows ("Adapters may
    // normalize editor-specific markup before the core policy"). `generic`
    // stays out: it takes arbitrary HTML, where a mutually linked anchor pair
    // is not proof of a footnote, and the caller naming an adapter is the
    // declaration of provenance that makes the recognition safe.
    let footnote_defs = importer.adapter_footnotes(
        &dom.document,
        matches!(
            options.adapter,
            HtmlImportAdapter::Word | HtmlImportAdapter::GoogleDocs
        ),
    )?;
    let children = importer.blocks(&fragment_top_level(&dom.document), "", 0)?;
    if writing {
        for (node, path, message) in std::mem::take(&mut importer.unspellable) {
            importer.diag(
                HtmlImportDiagnosticCode::StructureUnspellable,
                message,
                HtmlImportSeverity::Warning,
                &path,
                &node,
            );
        }
    }
    Ok(HtmlImportResult {
        value: Document {
            frontmatter: BTreeMap::new(),
            frontmatter_raw: None,
            footnote_defs,
            footnote_def_pos: BTreeMap::new(),
            children,
            source_len: 0,
            ingest_payload_len: 0,
        },
        report: HtmlImportReport {
            mode: options.mode,
            adapter: options.adapter,
            diagnostics: importer.report(),
        },
    })
}

pub fn html_to_carve(
    html: &str,
    options: &HtmlImportOptions,
) -> Result<HtmlImportResult<String>, HtmlImportError> {
    let result = import(html, options, true)?;
    let value = render_carve(&result.value).map_err(|error| match error {
        RenderCarveError::Depth(_) => HtmlImportError::RenderDepth,
        RenderCarveError::SourceUnspellable(_) => HtmlImportError::SourceUnspellable,
    })?;
    Ok(HtmlImportResult {
        value,
        report: result.report,
    })
}
