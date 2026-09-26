//! HTML5-to-Carve migration boundary.

use crate::ast::*;
use crate::escape::{
    decoded_style_value, has_denied_url_scheme, is_dangerous_attr_name, sanitize_attr_value,
};
use crate::extension::{
    label_default, HeadingIdOptions, LABEL_CODE_GROUP, LABEL_ENDNOTES, LABEL_INDEX_BACKREF,
    LABEL_TABS_GROUP,
};
use crate::profile::ADMONITION_TIER1_KINDS;
use crate::render::{semantic_value_target, EXTENDED_SEMANTIC_SPAN_ORDER};
use crate::render_carve::is_attr_identifier;
use crate::{render_carve, RenderCarveError};
use html5ever::tendril::TendrilSink;
use html5ever::{ns, QualName};
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
    /// An attribute the policy refused to represent as a Carve attribute, and
    /// that reached the output ANYWAY, inside the bytes of an element
    /// `roundtrip` keeps whole (markup-carve/carve-js#1468).
    ///
    /// NOT `AttributeDropped` carrying a different message. The two are
    /// opposite facts about the same attribute, and a consumer that filters on
    /// the code rather than reading the prose would be told a drop happened
    /// that did not - which is the row somebody acts on, because `roundtrip` is
    /// the mode `docs/html-import.md` calls unsafe for untrusted input.
    AttributePreserved => "attribute-preserved",
    StyleUnmapped => "style-unmapped",
    TableDegraded => "table-degraded",
    RawPreserved => "raw-preserved",
    /// A structure the AST holds and Carve 0.1 SOURCE has no spelling for, so
    /// only a WRITER loses it (PART 12 §16). Reported by `html_to_carve`;
    /// `html_to_ast` keeps the structure and says nothing.
    StructureUnspellable => "structure-unspellable",
    /// The source did not declare how to read a value and this importer picked
    /// an encoding anyway, so the node it produced is only correct if that
    /// guess holds.
    ///
    /// Distinct from `ElementUnwrapped`: unwrapping may lose a structural role
    /// while retaining text, whereas an assumed encoding warns that the output
    /// text itself is only inferred.
    EncodingAssumed => "encoding-assumed",
    DiagnosticsTruncated => "diagnostics-truncated",
});

report_vocabulary!(ImportFidelity {
    Preserved => "preserved",
    Normalized => "normalized",
    Degraded => "degraded",
    Dropped => "dropped",
});

report_vocabulary!(ImportConfidence {
    Exact => "exact",
    Inferred => "inferred",
    Fallback => "fallback",
});

impl HtmlImportDiagnosticCode {
    /// How much of the source survived this decision (importer fidelity v2).
    ///
    /// A property of the CODE, so it is answered here and every report layer
    /// reads it rather than deriving its own. The migration report used to
    /// carry the only copy, which left `html_to_carve`'s report - the one the
    /// shared fixtures describe - without a classification the format defines
    /// and this engine already knew.
    ///
    /// The match is exhaustive on purpose: a new code does not compile until
    /// it is classified, which is this engine's version of failing closed.
    pub fn fidelity(self) -> ImportFidelity {
        match self {
            Self::ElementDropped
            | Self::AttributeDropped
            | Self::StructureUnspellable
            | Self::DiagnosticsTruncated => ImportFidelity::Dropped,
            Self::ElementUnwrapped
            | Self::StyleUnmapped
            | Self::TableDegraded
            | Self::EncodingAssumed
            | Self::RawPreserved => ImportFidelity::Degraded,
            // PRESERVED and not DROPPED: it is the row that says an attribute
            // reached the output inside preserved raw bytes, so filing it
            // beside `AttributeDropped` would reintroduce the same false claim
            // one layer up (markup-carve/carve-js#1468).
            Self::AttributePreserved => ImportFidelity::Preserved,
        }
    }

    /// How sure the importer is that the decision was the right one.
    pub fn confidence(self) -> ImportConfidence {
        match self {
            // The importer assumed an encoding the source never declared.
            Self::EncodingAssumed => ImportConfidence::Inferred,
            // The report is a sample and the omitted findings are unknown.
            Self::DiagnosticsTruncated => ImportConfidence::Fallback,
            Self::ElementDropped
            | Self::ElementUnwrapped
            | Self::AttributeDropped
            | Self::AttributePreserved
            | Self::StyleUnmapped
            | Self::TableDegraded
            | Self::RawPreserved
            | Self::StructureUnspellable => ImportConfidence::Exact,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlImportDiagnostic {
    pub code: HtmlImportDiagnosticCode,
    pub message: String,
    pub severity: HtmlImportSeverity,
    /// Stamped from `code`, so a consumer never has to recompute it and can
    /// never disagree with the producer about it.
    pub fidelity: ImportFidelity,
    pub confidence: ImportConfidence,
    pub path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HtmlImportOptions {
    pub mode: HtmlImportMode,
    pub adapter: HtmlImportAdapter,
    /// Maximum input nesting, capped at [`MAX_HTML_IMPORT_DEPTH`] even when set higher.
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

/// Hard depth ceiling for HTML import, independent of caller options.
pub const MAX_HTML_IMPORT_DEPTH: usize = 128;

impl Default for HtmlImportOptions {
    fn default() -> Self {
        Self {
            mode: HtmlImportMode::Safe,
            adapter: HtmlImportAdapter::Generic,
            max_depth: MAX_HTML_IMPORT_DEPTH,
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

/// One row of the report, with what it takes to put it in the order the page
/// promises.
///
/// `owner` is set on an attribute `attrs` REFUSED, so the arm that keeps the
/// element as raw HTML can take the drop row back and report the attribute as
/// preserved instead (markup-carve/carve-js#1468).
struct DiagnosticEntry {
    at: usize,
    diagnostic: HtmlImportDiagnostic,
    owner: Option<Handle>,
}

/// One attribute the policy refused: what it IS (`subject`), why it was
/// refused (`reason`, empty where the subject says it), how loud the DROP is,
/// and whether the thing riding into preserved bytes is live.
struct Refusal {
    subject: String,
    reason: &'static str,
    severity: HtmlImportSeverity,
    live: bool,
}

struct Importer<'a> {
    opts: &'a HtmlImportOptions,
    /// Whether this import is the one that WRITES SOURCE (`html_to_carve`),
    /// which is the only exit allowed to record a source-layout field.
    ///
    /// PART 12 fixes `attrs.order` as a record of how a SOURCE spelled a block,
    /// and an import read HTML: there was no source to read a spelling off, so
    /// the PUBLISHED tree records none (markup-carve/carve#1647). The writer
    /// still has to be told that an imported heading id is AUTHORED - without
    /// that signal `render_carve` reads an id equal to its own generated slug
    /// as generated and omits it, which is the loss carve-rs#1324 closed by
    /// spelling the slot on both exits.
    ///
    /// So the slot is a WRITER-ONLY channel: the tree `html_to_carve` renders
    /// is an intermediate nobody publishes, and the tree `html_to_ast` returns
    /// carries no source-layout field at all.
    writing: bool,
    /// Every diagnostic, paired with the document position of the LOSING
    /// ELEMENT. The vector is built in construction order and sorted by that
    /// position on the way out, so a tie keeps the order the rows were built
    /// in - which for one element's attributes is the order it spells them.
    diagnostics: Vec<DiagnosticEntry>,
    /// The span of positions whose rows the diagnostics cap turned away, so
    /// `keep_raw` can withdraw the marker when all of them were inside it.
    turned_away: Option<(usize, usize)>,
    /// Every node of the parsed tree, numbered in DOCUMENT ORDER
    /// (markup-carve/carve#1586).
    document_order: HashMap<usize, (Handle, usize)>,
    nodes: usize,
    /// How many `<q>` elements are open around the node being read. HTML5
    /// leaves the marks to the user agent and every one of them alternates, so
    /// the depth is what chooses between the double and the single pair.
    quote_depth: usize,
    /// How many table cells the walk is inside.
    cell_depth: usize,
    /// The losses a WRITER takes, held back until one writes (PART 12 §16).
    /// The rows that belong to the WRITING exit only, each with the code it is
    /// reported under. `html_to_ast` keeps every structure these describe and is
    /// told nothing; `html_to_carve` flushes them once the walk is done, so they
    /// sort into document order with the rest of the report.
    unspellable: Vec<(Handle, String, String, HtmlImportDiagnosticCode)>,
    /// Where a figure's own attribute is DISPLACED by its target's (PART 12
    /// section 16, ruling markup-carve/carve#1721).
    displaced_figure_attrs: Vec<(Handle, String, String)>,
    /// Where the author wrote a `<p>` holding nothing but an image (PART 12
    /// section 16, markup-carve/carve-rs#1331).
    ///
    /// A CANDIDATE, not yet a loss. `block` is the only place the shape can be
    /// seen with a source path to report it against, and it runs BEFORE the
    /// unwrappers do: `caption_host` takes the paragraph back off a `<figure>`
    /// body, so the figure's target is the image on both exits and there is
    /// nothing left to lose. The row is emitted only for a candidate whose
    /// paragraph the FINISHED document still holds, which is why each one is
    /// marked in the tree rather than merely described here - two paragraphs
    /// around the same image are equal as values, and only a mark tells them
    /// apart.
    lone_image_paragraphs: Vec<LoneImageParagraph>,
    /// Every empty `<code>`, indexed by the mark on its node, so the writing
    /// exit can drop the ones whose backtick run does not end.
    empty_code_spans: Vec<(Handle, String)>,
    /// Every emphasis, insertion and deletion, indexed by the mark on its node,
    /// so the writing exit can unwrap a same-kind nesting the writer refuses.
    braced_kind_spans: Vec<(Handle, String)>,
    /// The reference sites the adapter footnote pass recognized: the node
    /// `inline` must read as a footnote reference, and the label it carries.
    ///
    /// Keyed by node ADDRESS, and the entry holds the handle it was keyed by -
    /// which is what makes the address a valid key. A key whose node had been
    /// dropped would be an address the allocator may hand to a live node next,
    /// so the map pins every node it can answer for.
    footnote_refs: HashMap<usize, (Handle, String)>,
    /// Fallback images of formulas that imported as math, dropped where they stand.
    formula_images: HashSet<usize>,
    sibling_index: SiblingIndex,
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

/// A cell's two alignment axes, as a `style` attribute states them.
///
/// Carried together rather than decided one at a time, because the MARKER RUN
/// that spells them is composed in one place and only reads correctly in one
/// order (PART 9 §5): the horizontal sigil first, the vertical second, with `?`
/// standing in for an inherited horizontal. A bare `|^` is not the vertical
/// spelling at all - it comes back as the literal text `^ a` - and `|~` ALONE
/// is the CENTER horizontal marker rather than a vertical one, so a run
/// assembled from two independent decisions has two ways to mean something
/// nobody wrote (markup-carve/carve#1746).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CellAlignment {
    align: Option<TableAlign>,
    valign: Option<TableVerticalAlign>,
}

/// The Carve slot a CSS declaration reaches, or `None` where nothing in the
/// language spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StyleSlot {
    Align(TableAlign),
    Valign(TableVerticalAlign),
}

/// A `style` attribute split into lowercased property/value pairs.
///
/// A declaration with no colon is not one, and an empty property name is not
/// one either, so `style=""`, `style=";;"` and `style="text-align"` all yield
/// nothing - which is what stops an attribute carrying no declaration at all
/// from reporting a loss it did not take.
fn style_declarations(style: &str) -> Vec<(String, String)> {
    style
        .split(';')
        .filter_map(|declaration| {
            let (property, value) = declaration.split_once(':')?;
            let property = property.trim().to_ascii_lowercase();
            if property.is_empty() {
                return None;
            }
            Some((property, value.trim().to_ascii_lowercase()))
        })
        .collect()
}

fn is_table_cell(tag: &str) -> bool {
    tag == "td" || tag == "th"
}

/// Each `url(...)` argument in a CSS value, with the quotes CSS allows around it
/// stripped. A quoted argument is read to its closing quote, so a `)` inside it
/// does not end the argument early.
fn css_url_arguments(value: &str) -> Vec<&str> {
    let lower = value.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(hit) = lower[from..].find("url(") {
        let open = from + hit + 4;
        let start = open + value[open..].len() - value[open..].trim_start().len();
        let rest = &value[start..];
        let (argument, after) = match rest.chars().next() {
            Some(quote @ ('"' | '\'')) => match rest[1..].find(quote) {
                Some(end) => (&rest[1..1 + end], start + 2 + end),
                None => break,
            },
            _ => match rest.find(')') {
                Some(end) => (&rest[..end], start + end),
                None => break,
            },
        };
        out.push(argument.trim());
        from = after;
    }
    out
}

/// How the refusal policy reads a `style`: the subject the row names, and
/// whether what the kept bytes carry is live (markup-carve/carve#2267).
///
/// The reason comes from a closed set of two, so it is derived rather than
/// chosen per call. Asking the renderer's own sanitizer rather than restating
/// its needles is what stops the importer refusing a different set than the
/// renderer blanks.
fn style_refusal(value: &str) -> (String, bool) {
    // THE SAME TEXT THE RENDERER ACTS ON, which is what makes the two readings
    // below one statement about one value rather than two scans that drift
    // (markup-carve/carve-rs#1921). On the raw bytes a `url(...)` inside a CSS
    // comment reported a denied scheme in a declaration the renderer writes
    // back untouched, and `url(java\73 cript:x)` was invisible to this half and
    // took the weaker of the two reasons markup-carve/carve#2267 allows.
    let declarations = decoded_style_value(value);
    if css_url_arguments(&declarations)
        .into_iter()
        .any(has_denied_url_scheme)
    {
        return (
            "style with a denied URL scheme in a declaration value".to_owned(),
            true,
        );
    }
    // BLANKED, not empty: the sanitizer answers `""` for `style=""` as well, so
    // the test is whether it changed the value.
    if sanitize_attr_value("style", value) != value {
        return (
            "style with a construct the CSS sanitizer refuses".to_owned(),
            true,
        );
    }
    ("style".to_owned(), false)
}

/// The slot a declaration reaches on an element of this tag, DISREGARDING the
/// import mode - [`Importer::style_slot`] is the one that answers for a mode.
fn mapped_style_slot(tag: &str, property: &str, value: &str) -> Option<StyleSlot> {
    match property {
        "text-align" => match value {
            "left" => Some(StyleSlot::Align(TableAlign::Left)),
            "right" => Some(StyleSlot::Align(TableAlign::Right)),
            "center" => Some(StyleSlot::Align(TableAlign::Center)),
            _ => None,
        },
        "vertical-align" if is_table_cell(tag) => match value {
            "top" => Some(StyleSlot::Valign(TableVerticalAlign::Top)),
            "middle" => Some(StyleSlot::Valign(TableVerticalAlign::Middle)),
            "bottom" => Some(StyleSlot::Valign(TableVerticalAlign::Bottom)),
            _ => None,
        },
        _ => None,
    }
}

/// The presentational attribute a slot supersedes.
fn style_slot_attribute_name(slot: StyleSlot) -> &'static str {
    match slot {
        StyleSlot::Align(_) => "align",
        StyleSlot::Valign(_) => "valign",
    }
}

fn align_keyword(align: TableAlign) -> &'static str {
    match align {
        TableAlign::Left => "left",
        TableAlign::Right => "right",
        TableAlign::Center => "center",
    }
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

/// Block-level elements with no Carve spelling. `roundtrip` keeps them as a
/// raw BLOCK, as carve-js and carve-php do (markup-carve/carve#2261); the other
/// modes still walk them as inline runs.
const ROUNDTRIP_RAW_BLOCK_TAGS: [&str; 4] = ["address", "fieldset", "form", "hgroup"];

/// The generic sectioning wrappers `roundtrip` UNWRAPS instead of preserving.
const ROUNDTRIP_UNWRAPPED_SECTIONING: [&str; 7] = [
    "article", "aside", "footer", "header", "main", "nav", "section",
];

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
        if depth > self.opts.max_depth.min(MAX_HTML_IMPORT_DEPTH) {
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
            let at = self.position_of(node);
            let displaced = self
                .diagnostics
                .last()
                .filter(|last| last.at != usize::MAX)
                .map_or(at, |last| last.at);
            let (low, high) = self.turned_away.unwrap_or((at, at));
            self.turned_away = Some((low.min(at).min(displaced), high.max(at).max(displaced)));
            if let Some(last) = self.diagnostics.last_mut() {
                // `usize::MAX`, so the marker stays where a reader needs it -
                // at the END of the report - rather than sorting to wherever
                // the row it replaced happened to sit.
                *last = DiagnosticEntry {
                    at: usize::MAX,
                    diagnostic: HtmlImportDiagnostic {
                        code: HtmlImportDiagnosticCode::DiagnosticsTruncated,
                        message: "HTML import diagnostics limit reached".into(),
                        severity: HtmlImportSeverity::Error,
                        fidelity: HtmlImportDiagnosticCode::DiagnosticsTruncated.fidelity(),
                        confidence: HtmlImportDiagnosticCode::DiagnosticsTruncated.confidence(),
                        path: None,
                    },
                    owner: None,
                };
            }
            return;
        }
        let at = self.position_of(node);
        self.diagnostics.push(DiagnosticEntry {
            at,
            diagnostic: HtmlImportDiagnostic {
                code,
                message,
                severity,
                fidelity: code.fidelity(),
                confidence: code.confidence(),
                path: Some(path.into()),
            },
            owner: None,
        });
    }

    /// An attribute this importer will not write as a Carve attribute. The row
    /// goes out as `attribute-dropped`, byte for byte the message the spec's
    /// `html-import` report fixtures pin; `keep_raw` takes it back where the
    /// element turns out to be kept whole.
    fn refuse_attribute(&mut self, node: &Handle, path: &str, tag: &str, refusal: Refusal) {
        let Refusal {
            subject,
            reason,
            severity,
            ..
        } = refusal;
        self.diag(
            HtmlImportDiagnosticCode::AttributeDropped,
            format!("Dropped {subject} on <{tag}>{reason}"),
            severity,
            path,
            node,
        );
        self.own_last(node, HtmlImportDiagnosticCode::AttributeDropped);
    }

    /// Mark the row just pushed as `node`'s, so `keep_raw` can take it back.
    ///
    /// NOT when the cap swallowed the row: `diag` replaces the last entry with
    /// the truncation marker there, which is why the code is checked.
    fn own_last(&mut self, node: &Handle, code: HtmlImportDiagnosticCode) {
        if let Some(entry) = self.diagnostics.last_mut() {
            if entry.diagnostic.code == code {
                entry.owner = Some(node.clone());
            }
        }
    }

    /// The element's OWN refused-attribute rows, restated as what the
    /// preserved bytes make them (markup-carve/carve-js#1468).
    fn record_displaced_figure_attrs(&mut self, node: &Handle, path: &str, figure: &Figure) {
        let Some(own) = figure.attrs.as_ref() else {
            return;
        };
        let theirs = match &*figure.target {
            FigureTarget::Image(_) => return,
            FigureTarget::BlockQuote(quote) => quote.attrs.as_ref(),
            FigureTarget::Table(table) => table.attrs.as_ref(),
            FigureTarget::CodeBlock(code) => code.attrs.as_ref(),
            FigureTarget::Paragraph(paragraph) => paragraph.attrs.as_ref(),
        };
        let Some(theirs) = theirs else {
            return;
        };
        let mut displaced: Vec<String> = Vec::new();
        if own.id.is_some() && theirs.id.is_some() {
            displaced.push("id".into());
        }
        for key in own.key_values.keys() {
            if theirs.key_values.contains_key(key) {
                displaced.push(key.clone());
            }
        }
        for name in displaced {
            self.displaced_figure_attrs
                .push((node.clone(), path.to_owned(), name));
        }
    }

    /// Report an element kept as raw HTML: its own refused attributes, its
    /// `raw-preserved` row, then the refused attributes of every element inside
    /// it in document order. All of them are in the kept bytes, so every one is
    /// `attribute-preserved`, and whatever the walk already reported about the
    /// descendants is taken back (markup-carve/carve#2261).
    fn keep_raw(&mut self, node: &Handle, path: &str, tag: &str) {
        let first = self.position_of(node);
        let mut descendants = Vec::new();
        let mut stack = vec![(node.clone(), path.to_owned())];
        while let Some((handle, handle_path)) = stack.pop() {
            let children = handle.children.borrow();
            for (index, child) in children.iter().enumerate().rev() {
                if Self::tag(child).is_some() {
                    stack.push((child.clone(), Self::child_path(&handle_path, child, index)));
                }
            }
            if !Rc::ptr_eq(&handle, node) {
                descendants.push((handle.clone(), handle_path));
            }
        }
        let last = descendants
            .iter()
            .map(|(handle, _)| self.position_of(handle))
            .filter(|at| *at != usize::MAX)
            .max()
            .unwrap_or(first);
        let inside = |at: usize| at > first && at <= last;
        self.diagnostics.retain(|entry| {
            !inside(entry.at)
                && !entry
                    .owner
                    .as_ref()
                    .is_some_and(|owner| Rc::ptr_eq(owner, node))
        });
        if self
            .turned_away
            .is_some_and(|(low, high)| inside(low) && inside(high))
        {
            self.turned_away = None;
            self.diagnostics.retain(|entry| {
                entry.diagnostic.code != HtmlImportDiagnosticCode::DiagnosticsTruncated
            });
        }
        let unspellable = std::mem::take(&mut self.unspellable);
        self.unspellable = unspellable
            .into_iter()
            .filter(|(handle, ..)| !inside(self.position_of(handle)))
            .collect();
        let displaced = std::mem::take(&mut self.displaced_figure_attrs);
        self.displaced_figure_attrs = displaced
            .into_iter()
            .filter(|(handle, ..)| !inside(self.position_of(handle)))
            .collect();
        self.report_kept_attributes(node, path, tag, None);
        self.diag(
            HtmlImportDiagnosticCode::RawPreserved,
            format!("Preserved unsupported <{tag}> element as raw HTML"),
            HtmlImportSeverity::Warning,
            path,
            node,
        );
        for (handle, handle_path) in descendants {
            let own = Self::tag(&handle).unwrap_or_default();
            self.report_kept_attributes(&handle, &handle_path, &own, Some(tag));
        }
    }

    /// One `attribute-preserved` row per refused attribute on `node`, in
    /// spelling order. `kept` names the raw-kept ancestor for a descendant.
    ///
    /// `Error` where the attribute is live in the output, which is the safety
    /// test the renderer applies: a refused sink, or any value its sanitizer
    /// would blank for a denied scheme. `Error` is not a failed import here;
    /// it is the only level that separates a preserved handler from a dropped
    /// one, which already spends `Warning`.
    fn report_kept_attributes(&mut self, node: &Handle, path: &str, tag: &str, kept: Option<&str>) {
        let NodeData::Element { attrs, .. } = &node.data else {
            return;
        };
        let style_filled = self.style_filled_attribute_names(node, tag);
        let place = match kept {
            None => "in the raw HTML this element is kept as".to_owned(),
            Some(kept) => format!("inside the raw HTML <{kept}> is kept as"),
        };
        let pairs: Vec<(String, String)> = attrs
            .borrow()
            .iter()
            .map(|attr| (attr.name.local.to_string(), attr.value.to_string()))
            .collect();
        for (name, value) in pairs {
            // The sanitizer's own test, so a denied token later in a URL list
            // counts too. `style` is exempt from BOTH halves: `refusal` reads it
            // declaration by declaration, where this probe would read
            // `background` as the scheme of `background:url(javascript:x)`.
            let denied = !name.eq_ignore_ascii_case("style")
                && (has_denied_url_scheme(&value)
                    || (!value.is_empty() && sanitize_attr_value(&name, &value).is_empty()));
            let (subject, reason, live) =
                match Self::refusal(node, tag, &name, &value, &style_filled) {
                    Some(refusal) => (refusal.subject, refusal.reason, refusal.live || denied),
                    None if denied => (format!("{name} with a denied URL scheme"), "", true),
                    None => continue,
                };
            self.diag(
                HtmlImportDiagnosticCode::AttributePreserved,
                format!("Preserved {subject} on <{tag}> {place}{reason}"),
                if live {
                    HtmlImportSeverity::Error
                } else {
                    HtmlImportSeverity::Info
                },
                path,
                node,
            );
        }
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
        entries.sort_by_key(|entry| entry.at);
        entries.into_iter().map(|entry| entry.diagnostic).collect()
    }
    /// The media wrappers whose children are FALLBACK CONTENT (ruling
    /// markup-carve/carve#1749).
    fn is_media_fallback_tag(tag: &str) -> bool {
        matches!(tag, "video" | "audio" | "object" | "canvas" | "picture")
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
    fn math_tex(&self, h: &Handle) -> Option<(u8, String)> {
        if let Some(annotated) = Self::tex_annotation(h) {
            return Some((1, annotated));
        }
        let alttext = Self::attr(h, "alttext").unwrap_or_default();
        if !alttext.trim().is_empty() {
            return Some((2, alttext.trim().to_string()));
        }
        // Adjacency alone is no evidence: the alt stands in for the formula
        // only where the page hid the MathML so the image renders instead.
        if !math_is_hidden(h) {
            return None;
        }
        let alt = Self::attr(&fallback_image(h, &self.sibling_index)?, "alt")?;
        let trimmed = alt.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some((3, trimmed.to_string()))
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
    /// `text()` without its recursion, for the annotation. It is read before
    /// the depth counter reaches its children, so this walk must stay iterative.
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
            label: None,
            number: None,
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
    /// The element's attribute names, in the source order html5ever kept them.
    fn element_attr_names(handle: &Handle) -> Vec<String> {
        match &handle.data {
            NodeData::Element { attrs, .. } => attrs
                .borrow()
                .iter()
                .map(|a| a.name.local.to_string())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Whether `id` sits where `render_heading` writes a GENERATED one: after
    /// every authored attribute. `data-source-line` is the one thing that
    /// follows it, because that is a render annotation rather than an authored
    /// attribute and `render_heading` emits it last on purpose.
    fn id_in_generated_position(handle: &Handle) -> bool {
        let mut names = Self::element_attr_names(handle);
        while names.last().is_some_and(|name| name == "data-source-line") {
            names.pop();
        }
        names.last().is_some_and(|name| name == "id")
    }

    /// Whether `id` is a value the renderer would derive for a heading whose
    /// plain-text projection is `text`.
    ///
    /// THE DEFAULT SLUG ONLY, which is the same accepted limit `drop_derived`
    /// states for every other derived attribute: an importer cannot know which
    /// `HeadingIdOptions` the render used, and a value no default equals is
    /// indistinguishable from an authored one, so failing SAFE - keep - is the
    /// side to err on. The `-N` tail is `next_heading_id`'s own dedup shape,
    /// which starts at 2 because the first occurrence takes the bare base.
    fn is_generated_heading_id(id: &str, text: &str) -> bool {
        let base = crate::parse::slugify_parse(text, HeadingIdOptions::PLAIN);
        if id == base {
            return true;
        }
        id.strip_prefix(&base)
            .and_then(|rest| rest.strip_prefix('-'))
            .is_some_and(|count| {
                !count.is_empty()
                    && !count.starts_with('0')
                    && count != "1"
                    && count.bytes().all(|b| b.is_ascii_digit())
            })
    }

    /// The writer's slot order for `held`, read off the element's own attribute
    /// order. The importer used to spell a fixed id-then-class-then-keys order,
    /// which renders `{.k #x}` back as `<h1 id="x" class="k">` - attributes the
    /// input did not have in that order (carve-rs#1354).
    fn slot_order_from_element(handle: &Handle, held: &Attrs) -> Vec<AttrSlot> {
        let mut order: Vec<AttrSlot> = Vec::new();
        for name in Self::element_attr_names(handle) {
            let slot = match name.as_str() {
                "id" if held.id.is_some() => AttrSlot::Id,
                "class" if !held.classes.is_empty() => AttrSlot::Class,
                other if held.key_values.contains_key(other) => AttrSlot::Key(other.to_string()),
                _ => continue,
            };
            if !order.contains(&slot) {
                order.push(slot);
            }
        }
        // A non-empty order is EXHAUSTIVE, so anything the element did not
        // spell under its own name - an attribute renamed or folded on the way
        // in - still has to appear, or the writer drops it silently.
        if held.id.is_some() && !order.contains(&AttrSlot::Id) {
            order.push(AttrSlot::Id);
        }
        if !held.classes.is_empty() && !order.contains(&AttrSlot::Class) {
            order.push(AttrSlot::Class);
        }
        for key in held.key_values.keys() {
            let slot = AttrSlot::Key(key.clone());
            if !order.contains(&slot) {
                order.push(slot);
            }
        }
        order
    }

    /// The element's `title` folded back into its attributes. A link and an
    /// image carry it in their own slot, so the attribute walk refuses it
    /// (carve-rs#1060); an element naming no destination has no slot to carry
    /// it, and the contract keeps the attributes that survive (carve-rs#1738).
    fn attrs_with_title(handle: &Handle, attrs: Option<Attrs>) -> Option<Attrs> {
        let Some(title) = Self::attr(handle, "title") else {
            return attrs;
        };
        let mut held = attrs.unwrap_or_default();
        held.key_values.insert("title".to_string(), title);
        // An EMPTY order is the writer's own id-classes-keys fallback; a
        // non-empty one is exhaustive, so the new key has to join it or the
        // writer drops every attribute the element did spell.
        let slot = AttrSlot::Key("title".to_string());
        if !held.order.is_empty() && !held.order.contains(&slot) {
            held.order.push(slot);
        }
        Some(held)
    }

    /// The slot a declaration reaches IN THIS MODE.
    ///
    /// `safe` MAPS NOTHING. It is the conservative mode: a declaration it
    /// declines is dropped and reported, which is where every mode stood before
    /// markup-carve/carve#1741 and where `safe` stays after it.
    fn style_slot(&self, tag: &str, property: &str, value: &str) -> Option<StyleSlot> {
        if self.opts.mode == HtmlImportMode::Safe {
            return None;
        }
        mapped_style_slot(tag, property, value)
    }

    /// The axes a CELL's `style` puts on the cell itself.
    ///
    /// Read off the element a second time rather than handed over from
    /// [`Self::attrs`], because the two answers go to different places: an
    /// alignment is a FIELD of the cell, spelled by the marker run, while
    /// everything `attrs` collects is an attribute block. Both walk the same
    /// [`Self::style_slot`], so the report and the cell cannot disagree about
    /// what mapped.
    ///
    /// LAST DECLARATION WINS, which is the CSS cascade: `text-align: right;
    /// text-align: left` is left-aligned in a browser and has to be here.
    fn cell_style_alignment(&self, handle: &Handle) -> CellAlignment {
        let tag = Self::tag(handle).unwrap_or_default();
        let mut out = CellAlignment::default();
        if !is_table_cell(&tag) {
            return out;
        }
        let style = Self::attr(handle, "style").unwrap_or_default();
        for (property, value) in style_declarations(&style) {
            match self.style_slot(&tag, &property, &value) {
                Some(StyleSlot::Align(align)) => out.align = Some(align),
                Some(StyleSlot::Valign(valign)) => out.valign = Some(valign),
                None => {}
            }
        }
        out
    }

    /// The presentational attribute names a mapped declaration on this element
    /// already fills.
    ///
    /// Read BEFORE the attribute walk, and that is the whole point. CSS beats
    /// the presentational attribute in HTML, and it has to beat it in BOTH
    /// source orders: answering this as the walk reached `style` would let
    /// `<td align="right" style="text-align:left">` keep both, because the
    /// attribute had already been written by then.
    fn style_filled_attribute_names(&self, handle: &Handle, tag: &str) -> BTreeSet<&'static str> {
        let style = Self::attr(handle, "style").unwrap_or_default();
        style_declarations(&style)
            .into_iter()
            .filter_map(|(property, value)| self.style_slot(tag, &property, &value))
            .map(style_slot_attribute_name)
            .collect()
    }

    /// Whether `data-task-state` IS the item's state: one PART 10 §11 writes,
    /// on an EMPTY box. Anything else is the author's attribute.
    fn reads_task_state(li: &Handle) -> bool {
        let Some(state) = Self::attr(li, "data-task-state") else {
            return false;
        };
        if !matches!(state.as_str(), "-" | "_" | ">" | "?") {
            return false;
        }
        li.children.borrow().iter().any(|child| {
            Self::tag(child).as_deref() == Some("input")
                && Self::attr(child, "type").is_some_and(|t| t.eq_ignore_ascii_case("checkbox"))
                && Self::attr(child, "checked").is_none()
        })
    }

    /// Why the attribute policy refuses `name`, or `None` where `attrs` keeps
    /// it or the branch building the node consumes it. The raw-kept report asks
    /// the same question of every element inside the kept bytes, so both read
    /// this one answer.
    fn refusal(
        handle: &Handle,
        tag: &str,
        name: &str,
        value: &str,
        style_filled: &BTreeSet<&'static str>,
    ) -> Option<Refusal> {
        // THE REFUSAL IS DERIVED, NOT ENUMERATED. `is_dangerous_attr_name`
        // is the PART 9 §25 name filter the HTML renderer already
        // applies, so the importer refuses exactly what the renderer
        // would blank and the two cannot drift apart. Spelling a second
        // `starts_with("on")` here agreed on handlers and diverged on
        // `srcdoc` and `formaction`, which the renderer refuses and the
        // importer used to keep as "unsupported" by accident of the
        // keep list ending before them (carve-rs#1060).
        if is_dangerous_attr_name(name) {
            // The handler wording is a SHARED CONTRACT: the spec's
            // `html-import` report fixtures pin it byte for byte, so
            // the two sinks the filter adds are named separately rather
            // than folded into one message that would move it.
            // `injection-sink`, not a local synonym: `docs/security.md`
            // calls `srcdoc` and `formaction` sinks, and carve-js and
            // carve-php both spell the subject that way.
            let what = if name.to_ascii_lowercase().starts_with("on") {
                "event-handler"
            } else {
                "injection-sink"
            };
            return Some(Refusal {
                subject: format!("{what} attribute {name}"),
                reason: "",
                severity: HtmlImportSeverity::Warning,
                live: true,
            });
        }
        // The VALUE of a compact semantic span (PART 9 §10), `id` and `class`
        // are read by `attrs` itself.
        if matches!(name, "id" | "class")
            || (is_semantic_span_tag(tag) && semantic_value_target(tag) == Some(name))
        {
            return None;
        }
        if name == "style" {
            // THE SAME POLICY AS EVERY OTHER ATTRIBUTE (markup-carve/carve#2267).
            // The CSS mapping in `attrs` answers for a document the import
            // rewrites; this answers for bytes kept whole, where no mapping runs
            // and `style-unmapped` would name one that did.
            let (subject, live) = style_refusal(value);
            return Some(Refusal {
                subject,
                reason: "",
                severity: if live {
                    HtmlImportSeverity::Error
                } else {
                    HtmlImportSeverity::Info
                },
                live,
            });
        }
        if style_filled.contains(name) {
            // SUPERSEDED BY THE CSS BESIDE IT. A browser does not read
            // `<td style="text-align:left" align="right">` as
            // right-aligned just because `align` was written second, so
            // keeping both would spell one axis twice, in two
            // spellings, from one source - and the two would disagree.
            return Some(Refusal {
                subject: format!("attribute {name}"),
                reason: ": a mapped CSS declaration already sets it",
                severity: HtmlImportSeverity::Info,
                live: false,
            });
        }
        if Self::is_consumed_attribute(handle, tag, name) {
            return None;
        }
        if name == "data-djot-src" || name == "data-carve-src" {
            // The round-trip provenance markers this engine WRITES.
            // Reading one back as an ordinary attribute would let an
            // import restate a source the document no longer has.
            return Some(Refusal {
                subject: format!("round-trip marker attribute {name}"),
                reason: "",
                severity: HtmlImportSeverity::Info,
                live: false,
            });
        }
        if is_semantic_span_tag(tag) && name == tag {
            // THE MARKER OWNS THIS KEY. A compact semantic span is
            // written `[t]{cite}`, so the tag name becomes an attribute
            // key on the way out. An element carrying an attribute of
            // its own name would have it overwritten by that marker,
            // silently: `<cite cite="https://x">` stored the key twice
            // and the URL lost to the empty marker value. The keep list
            // hid this by refusing `cite` on everything but a
            // `<blockquote>`; naming the drop is the honest form
            // (carve-rs#1060).
            return Some(Refusal {
                subject: format!("attribute {name}"),
                reason: ": the semantic span's marker owns that key",
                severity: HtmlImportSeverity::Info,
                live: false,
            });
        }
        if name == "srcset" {
            return Some(Refusal {
                subject: format!("list-valued URL attribute {name}"),
                reason: "",
                severity: HtmlImportSeverity::Warning,
                live: sanitize_attr_value(name, value).is_empty() && !value.is_empty(),
            });
        }
        if !is_attr_identifier(name) {
            // No BARE spelling in Carve attribute syntax. The writer's
            // `escape_attr_key` strips every character the rule
            // rejects, so keeping `xlink:href` would emit `xlinkhref`
            // and the document would claim an attribute nobody wrote.
            // Losing it loudly beats renaming it quietly.
            // Refused for the shape of its NAME, so the value is what
            // decides whether the preserved bytes carry something live.
            // The wording is RULED (carve-rs#1882), not a local choice: the
            // three engines each picked a different half of it, and this form
            // is the one matching both patterns the report already uses -
            // `unsupported` for "Carve has no spelling for this", and
            // colon-plus-reason for explaining a refusal.
            return Some(Refusal {
                subject: format!("unsupported attribute {name}"),
                reason: ": not spellable as a Carve attribute name",
                severity: HtmlImportSeverity::Info,
                live: sanitize_attr_value(name, value).is_empty() && !value.is_empty(),
            });
        }
        None
    }

    /// CONSUMED by the branch that builds this node, and written back from
    /// there. Keeping it as an attribute as well would spell the same string
    /// twice, and diagnosing it would name a loss that does not happen.
    fn is_consumed_attribute(handle: &Handle, tag: &str, name: &str) -> bool {
        matches!(
            (tag, name),
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
        ) || (tag == "li" && name == "data-task-state" && Self::reads_task_state(handle))
    }

    /// The CSS mapping: every declaration that reaches a slot, and one
    /// `style-unmapped` row for the ones that reach none.
    ///
    /// ONLY THE DECLARATIONS THAT WENT NOWHERE. `style` used to be refused
    /// wholesale, so a cell carrying `text-align:right` came back unaligned AND
    /// carrying a row naming a loss this engine does not have to take - the
    /// alignment has somewhere faithful to go, and `docs/html-import.md` makes a
    /// declared loss a ceiling rather than a license (markup-carve/carve#1741).
    ///
    /// The row is OWNED by its element, which is how `keep_raw` takes it back
    /// where the kept bytes run no mapping for it to describe.
    fn map_style(&mut self, handle: &Handle, path: &str, tag: &str, value: &str, out: &mut Attrs) {
        let cell = is_table_cell(tag);
        let mut unmapped = false;
        for (property, val) in style_declarations(value) {
            match self.style_slot(tag, &property, &val) {
                // A CELL TAKES THE MARKER RUN, NOT AN ATTRIBUTE. `|>` renders
                // back as `style="text-align: right;"` and `{align=right}` as
                // `align="right"`, so only the marker returns the declaration
                // the import was handed - and only the marker keeps
                // `carve -> html -> carve -> html` a fixed point, which the
                // key-value was not (markup-carve/carve#1745). The cell's own
                // fields carry it; `cell_style_alignment` reads them off the
                // same element.
                Some(_) if cell => {}
                // OFF A CELL there is no marker run, and `align` is a legacy
                // presentational attribute HTML defines for exactly these
                // elements, so the key-value is the faithful spelling rather
                // than a second-best one. `vertical-align` reaches no slot here
                // at all and so never takes this arm.
                Some(StyleSlot::Align(align)) => {
                    out.key_values
                        .insert("align".to_string(), align_keyword(align).to_string());
                }
                _ => unmapped = true,
            }
        }
        if unmapped {
            self.diag(
                HtmlImportDiagnosticCode::StyleUnmapped,
                "CSS declarations were not mapped".into(),
                HtmlImportSeverity::Info,
                path,
                handle,
            );
            self.own_last(handle, HtmlImportDiagnosticCode::StyleUnmapped);
        }
    }

    fn attrs(&mut self, handle: &Handle, path: &str) -> Option<Attrs> {
        let tag = Self::tag(handle).unwrap_or_default();
        let mut out = Attrs::default();
        // The keys a mapped declaration fills, so a presentational `align` /
        // `valign` beside one can be refused wherever the source put it.
        let style_filled = self.style_filled_attribute_names(handle, &tag);
        if let NodeData::Element { attrs, .. } = &handle.data {
            for attr in attrs.borrow().iter() {
                let name = attr.name.local.to_string();
                let value = attr.value.to_string();
                if name == "style" {
                    // AHEAD OF THE REFUSAL, deliberately. `refusal` answers for
                    // `style` so the kept-bytes reading goes through the shared
                    // policy, and this element is one the import rewrites - the
                    // CSS mapping is its reading, not a drop.
                    self.map_style(handle, path, &tag, &value, &mut out);
                } else if let Some(refusal) =
                    Self::refusal(handle, &tag, &name, &value, &style_filled)
                {
                    self.refuse_attribute(handle, path, &tag, refusal);
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
                } else if Self::is_consumed_attribute(handle, &tag, &name) {
                    // Written back by the branch that builds the node.
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
        let tag = Self::tag(child).unwrap_or_else(|| {
            if matches!(&child.data, NodeData::Comment { .. }) {
                "comment()".into()
            } else {
                "text()".into()
            }
        });
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
            let path = match paths {
                Some(given) => given[i].clone(),
                None => Self::child_path(parent, handle, i),
            };
            self.block_item(
                handle,
                path,
                parent,
                depth,
                &mut out,
                &mut inline,
                &mut inline_paths,
            )?;
        }
        if !inline.is_empty() {
            self.flush_inline_run(&mut out, &mut inline, &mut inline_paths, parent, depth)?;
        }
        Ok(out)
    }

    /// One child of a block walk: emitted as a block, or buffered into the
    /// current inline run.
    #[allow(clippy::too_many_arguments)]
    fn block_item(
        &mut self,
        handle: &Handle,
        path: String,
        parent: &str,
        depth: usize,
        out: &mut Vec<BlockNode>,
        inline: &mut Vec<Handle>,
        inline_paths: &mut Vec<String>,
    ) -> Result<(), HtmlImportError> {
        // AN UNSUPPORTED ELEMENT IS REPLACED BY ITS CHILDREN IN PLACE
        // (markup-carve/carve#2341), so they join this walk as if the
        // wrapper were absent: blocks stay blocks, inline text joins the
        // surrounding run.
        if self.is_unmapped_element(handle) {
            return self.splice_unmapped(handle, path, parent, depth, out, inline, inline_paths);
        }
        let tag = Self::tag(handle);
        let is_block = tag.as_deref().is_some_and(|tag| {
            Self::is_block_tag(tag)
                || (self.opts.mode == HtmlImportMode::Roundtrip
                    && ROUNDTRIP_RAW_BLOCK_TAGS.contains(&tag))
        });
        // A MEDIA WRAPPER'S FALLBACK IS CONVERTED AS BLOCKS (ruling
        // markup-carve/carve#1749). It is not a block tag and must not
        // become one - see `is_media_fallback_tag` for why `roundtrip` keeps
        // the inline raw span - so the ruling is applied here, at the one
        // position where the children have somewhere block-shaped to go.
        let is_media_fallback = self.opts.mode != HtmlImportMode::Roundtrip
            && tag
                .as_deref()
                .map(Self::is_media_fallback_tag)
                .unwrap_or(false);
        if is_block || is_media_fallback {
            if !inline.is_empty() {
                self.flush_inline_run(out, inline, inline_paths, parent, depth)?;
            }
            if is_media_fallback {
                let tag = tag.expect("a media fallback tag is an element name");
                out.extend(self.media_fallback(handle, &tag, &path, depth + 1)?);
            } else {
                out.extend(self.block(handle, &path, depth + 1)?);
            }
        } else {
            inline.push(handle.clone());
            inline_paths.push(path);
        }
        Ok(())
    }

    /// Whether a `<p>` is among the children, looking through the unsupported
    /// elements the walk splices away.
    fn holds_paragraph(&self, h: &Handle) -> bool {
        h.children.borrow().iter().any(|child| {
            Self::tag(child).as_deref() == Some("p")
                || (self.is_unmapped_element(child) && self.holds_paragraph(child))
        })
    }

    /// Whether the inline arm would unwrap this element for want of a mapping.
    /// `roundtrip` keeps such an element raw instead, so it never splices.
    fn is_unmapped_element(&self, h: &Handle) -> bool {
        if self.opts.mode == HtmlImportMode::Roundtrip
            || self.footnote_refs.contains_key(&node_key(h))
        {
            return false;
        }
        let Some(tag) = Self::tag(h) else {
            return false;
        };
        !(Self::is_block_tag(&tag)
            || Self::is_media_fallback_tag(&tag)
            || is_semantic_span_tag(&tag)
            || matches!(
                tag.as_str(),
                "script"
                    | "style"
                    | "template"
                    | "noscript"
                    | "q"
                    | "math"
                    | "span"
                    | "em"
                    | "i"
                    | "strong"
                    | "b"
                    | "s"
                    | "strike"
                    | "ins"
                    | "del"
                    | "u"
                    | "mark"
                    | "sub"
                    | "sup"
                    | "code"
                    | "a"
                    | "img"
                    | "br"
            ))
    }

    /// The unsupported element's rows, then its children walked in its place.
    /// The rows are the inline arm's, so the report does not depend on which
    /// context the element stood in.
    #[allow(clippy::too_many_arguments)]
    fn splice_unmapped(
        &mut self,
        h: &Handle,
        path: String,
        parent: &str,
        depth: usize,
        out: &mut Vec<BlockNode>,
        inline: &mut Vec<Handle>,
        inline_paths: &mut Vec<String>,
    ) -> Result<(), HtmlImportError> {
        let depth = depth + 1;
        self.enter(depth)?;
        let tag = Self::tag(h).expect("an unmapped element has a name");
        let attrs = self.attrs(h, &path);
        let children: Vec<Handle> = h.children.borrow().iter().cloned().collect();
        for (i, child) in children.iter().enumerate() {
            let child_path = Self::child_path(&path, child, i);
            self.block_item(child, child_path, parent, depth, out, inline, inline_paths)?;
        }
        let unwrapped = self.report_unsupported_element(h, &tag, &path);
        self.report_unplaceable_attrs(
            h,
            attrs,
            &tag,
            if unwrapped {
                "the element was unwrapped and has no node to carry it"
            } else {
                "the empty element was dropped and has no node to carry it"
            },
            &path,
        );
        Ok(())
    }

    /// A media wrapper standing among blocks, unwrapped to its fallback
    /// (ruling markup-carve/carve#1749).
    ///
    /// THE SAME TWO ROWS THE UNMAPPED ARM WRITES, deliberately: the row for the
    /// element, the rows for the attributes it could not carry, then the
    /// children as BLOCKS. What changes is only that the children go through
    /// `blocks_at` rather than the inline flatten, so a `<p>` inside stays a
    /// paragraph, a `<ul>` stays a list and an `<h2>` stays a heading, as
    /// carve-php has always written them.
    ///
    /// THE INNER ROWS FALL OUT RATHER THAN BEING PORTED. Flattening reported an
    /// `element-unwrapped` for every block it dissolved, and those rows were
    /// truthful about that output; a `<p>` that survives as a paragraph is not
    /// unwrapped and owes none. A fix that kept them while changing the
    /// conversion would start making false statements.
    ///
    /// `roundtrip` NEVER REACHES HERE. Its answer for these elements is the raw
    /// inline span the inline arm writes, which is what all three engines
    /// produce and what this ruling does not touch.
    fn media_fallback(
        &mut self,
        h: &Handle,
        tag: &str,
        path: &str,
        depth: usize,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let attrs = self.attrs(h, path);
        let unwrapped = self.report_unsupported_element(h, tag, path);
        self.report_unplaceable_attrs(
            h,
            attrs,
            tag,
            if unwrapped {
                "the element was unwrapped and has no node to carry it"
            } else {
                "the empty element was dropped and has no node to carry it"
            },
            path,
        );
        let children: Vec<Handle> = h.children.borrow().iter().cloned().collect();

        self.blocks_at(&children, None, path, depth)
    }

    /// The buffered inline run, emitted as whatever it turns out to be.
    ///
    /// A RUN THAT HOLDS NOTHING BUT COMMENTS IS A BLOCK COMMENT RUN, not a
    /// paragraph carrying inline ones (markup-carve/carve#1709).
    ///
    /// The POSITION decides a comment's spelling and the comment is not
    /// relocated, and this is where the two positions are told apart.
    /// `blocks_at` buffers every non-block node into an inline run, so
    /// `<p>a</p><!--n--><p>b</p>` arrives here as a run holding one comment and
    /// nothing else - which is a comment sitting AMONG BLOCKS, however the
    /// buffer got it here. A run that also carries content is a real inline run
    /// and its comment is inline: `<div>text <!--n--> more</div>` is ONE
    /// paragraph, and splitting it at the comment would move the words either
    /// side of it into two.
    ///
    /// Whitespace-only text is not "something else". It is the layout between
    /// the blocks, which is exactly what a comment between two of them sits in,
    /// and counting it as content would make the answer depend on whether the
    /// author indented their HTML.
    fn flush_inline_run(
        &mut self,
        out: &mut Vec<BlockNode>,
        inline: &mut Vec<Handle>,
        inline_paths: &mut Vec<String>,
        parent: &str,
        depth: usize,
    ) -> Result<(), HtmlImportError> {
        let comments_only = inline
            .iter()
            .any(|h| matches!(&h.data, NodeData::Comment { .. }))
            && inline
                .iter()
                .all(|h| matches!(&h.data, NodeData::Comment { .. }) || dom_text_is_layout_only(h));
        if comments_only {
            for handle in inline.iter() {
                if let NodeData::Comment { contents } = &handle.data {
                    out.push(BlockNode::Comment(Comment {
                        block: true,
                        delimited: false,
                        content: contents.to_string(),
                        pos: None,
                    }));
                }
            }
            inline.clear();
            inline_paths.clear();
            return Ok(());
        }
        let children = self.inlines_at(inline, Some(inline_paths), parent, depth + 1)?;
        if visible(&children) {
            out.push(synthesized_wrapper(trim_edge_whitespace(children)));
        }
        inline.clear();
        inline_paths.clear();
        Ok(())
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
            return Ok(vec![BlockNode::Directive(Directive {
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
            let mut attrs = attrs;
            let inlines = self.inlines(&children, path, depth + 1)?;
            if let Some(held) = attrs.as_mut().filter(|held| held.id.is_some()) {
                if self.opts.mode == HtmlImportMode::Roundtrip
                    && Self::id_in_generated_position(h)
                    && held.id.as_deref().is_some_and(|id| {
                        Self::is_generated_heading_id(id, &crate::render::plain_inlines(&inlines))
                    })
                {
                    // The renderer derives it again from the same text, so
                    // dropping it is the no-op `drop_derived` documents for
                    // every other derived attribute. Carrying it would spell an
                    // authored slot the source never had.
                    held.id = None;
                } else if self.writing {
                    // Authored. A non-empty order is exhaustive, so every
                    // imported slot has to be carried - and carried in the
                    // element's OWN attribute order, which is the order the
                    // writer has to spell to render these bytes back.
                    //
                    // ON THE WRITING EXIT ONLY. `order` is a source-layout
                    // field and an import read no source, so the published tree
                    // records none of them (markup-carve/carve#1647); see
                    // `Importer::writing` for why the writer still needs it.
                    held.order = Self::slot_order_from_element(h, held);
                }
            }
            let attrs = attrs.filter(|held| *held != Attrs::default());
            return Ok(vec![BlockNode::Heading(Heading {
                attrs,
                level: tag[1..].parse().unwrap(),
                children: inlines,
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
            /*
             * CARVE SOURCE CANNOT SPELL A PARAGRAPH HOLDING ONLY AN IMAGE, so
             * the writer loses it and PART 12 section 16 says the writing exit
             * reports that (carve-rs#1331).
             *
             * `resources/examples/edge-cases.md` rules the shape: "a paragraph
             * whose whole content is one image is still the standalone image
             * shape, not a wrapped one". So `![G](g.jpg)` re-reads as a BLOCK
             * image, the `<p>` the author wrote is gone from the source, and
             * `html_to_ast` keeps a paragraph the re-parsed source does not -
             * the one carve-out `docs/html-import.md` allows to
             * `parse(html_to_carve(h)) == html_to_ast(h)`.
             *
             * NOT A CHANGE OF OUTPUT, because there is no other output to
             * write. carve-js measured an indented ` ![G](g.jpg)` as a
             * paragraph holding one image and rejected it as a spelling anyway;
             * this engine reads it as a block image at every indent, so there
             * is not even a near miss to reach for.
             */
            // THE AUTHOR'S PARAGRAPH IS TRIMMED TOO (carve-rs#1336). The
            // synthesized wrapper beside it already was, and the reason given -
            // that the wrapper is not an element the document contains - is real
            // but is not the deciding one. PART 11 section 7's principle is:
            // whitespace that is layout is not content, wherever it sits. Left
            // untrimmed, `<p>` newline `  <img>` newline `</p>` kept two spaces
            // in the tree that the writer drops, so the two exits disagreed
            // about characters no reader can act on.
            let inlines = trim_edge_whitespace(inlines);
            let candidate = lone_image(&inlines).map(|image| {
                (
                    attrs.is_some(),
                    overwritten_attr_names(attrs.as_ref(), image.attrs.as_ref()),
                )
            });
            let mut paragraph = Paragraph {
                attrs,
                children: inlines,
                at_content_column: true,
                block_image: false,
                pos: None,
            };
            if let Some((attributed, overwritten)) = candidate {
                paragraph.pos = Some(candidate_mark(self.lone_image_paragraphs.len()));
                self.lone_image_paragraphs.push(LoneImageParagraph {
                    node: h.clone(),
                    path: path.to_string(),
                    attributed,
                    overwritten,
                });
            }
            return Ok(vec![BlockNode::Paragraph(paragraph)]);
        }
        if tag == "blockquote" {
            return Ok(vec![BlockNode::BlockQuote(BlockQuote {
                attrs,
                children: self.blocks(&children, path, depth + 1)?,
                // An imported quote has no authored spelling: HTML records the
                // element, never how someone typed it.
                fenced: false,
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
            let mut content = Self::text(code);
            if content.ends_with('\n') {
                content.pop();
            }
            return Ok(vec![BlockNode::CodeBlock(CodeBlock {
                attrs,
                lang,
                title: None,
                label: None,
                content,
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
                // `is_layout_space` AND NOT `str::trim`: `trim` is
                // `char::is_whitespace`, so a stray U+00A0 read as blank and the
                // move went UNDECLARED while the same move for an ordinary word
                // was reported (markup-carve/carve-rs#1342). The character was
                // kept either way - what was missing is the row saying it left
                // its place among the items, which is the part a reader cannot
                // see from the output.
                } else if matches!(&child.data, NodeData::Comment { .. }) {
                    self.diag(
                        HtmlImportDiagnosticCode::ElementUnwrapped,
                        format!(
                            "An HTML comment directly inside <{tag}> kept its text but not its place among the items: it is emitted as a comment ahead of the list"
                        ),
                        HtmlImportSeverity::Info,
                        &p,
                        child,
                    );
                } else if !Self::text(child).chars().all(is_layout_space) {
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
                let li_children = li.children.borrow();
                let checkbox = li_children.iter().enumerate().find(|(_, child)| {
                    Self::tag(child).as_deref() == Some("input")
                        && Self::attr(child, "type")
                            .is_some_and(|value| value.eq_ignore_ascii_case("checkbox"))
                });
                // PART 10 §11's carrier, read before the marker text so an
                // ordered item keeps the state character it was written with
                // rather than defaulting to the box.
                let task_state = Self::reads_task_state(li)
                    .then(|| Self::attr(li, "data-task-state"))
                    .flatten()
                    .and_then(|state| state.chars().next());
                // UNSPELLABLE BEHIND AN ORDERED MARKER. `task_marker` in
                // `resources/spec/03-blocks-core.ebnf` hangs off
                // `unordered_item` alone, so no Carve source carries a box on an
                // ordered item and the writer dropped both the box and the
                // characters it was read from (carve-rs#1890). The bracket pair
                // is kept as text, the way the Markdown importer keeps it
                // (carve-rs#1886), and the row says what could not be spelled.
                // The AST holds the box either way; only a writer loses it
                // (PART 12 §16), so this runs on the writing exit alone.
                let ordered_task = ordered && checkbox.is_some() && self.writing;
                if let Some((j, input)) = checkbox {
                    let input_path = Self::child_path(&p, input, j);
                    if ordered_task {
                        self.diag(
                            HtmlImportDiagnosticCode::StructureUnspellable,
                            ORDERED_TASK_ITEM_UNSPELLABLE.into(),
                            HtmlImportSeverity::Warning,
                            &input_path,
                            input,
                        );
                    }
                    if let Some(kept) = self.attrs(input, &input_path) {
                        let names: Vec<String> = Self::attr_names(&kept)
                            .into_iter()
                            .filter(|name| {
                                !matches!(
                                    name.as_str(),
                                    "type" | "checked" | "disabled" | "aria-label"
                                )
                            })
                            .collect();
                        if !names.is_empty() {
                            self.diag(
                                HtmlImportDiagnosticCode::AttributeDropped,
                                format!(
                                    "Dropped {} on <input>: a task item's checkbox has no attribute slot",
                                    names.join(", ")
                                ),
                                HtmlImportSeverity::Warning,
                                &input_path,
                                input,
                            );
                        }
                    }
                }
                // CONSUMED INTO THE MARKER, so it is not walked as content and
                // leaves no `element-unwrapped` or `attribute-dropped` behind
                // it - reporting the `type` and `checked` lost would name a
                // loss that no longer happens.
                //
                // The siblings after it keep the index they have in the
                // DOCUMENT rather than the one they take in the filtered array,
                // which is why the paths are computed here and handed to
                // `blocks_at`: rebuilding an index from a list something was
                // lifted out of renumbers every sibling after the hole
                // (PART 12 §16, markup-carve/carve#1554).
                //
                // An ordered item's box is the one case where it is not
                // consumed but SUBSTITUTED: a text node standing where the
                // `<input>` stood, so the bracket pair reaches the item's own
                // inline run and keeps the position the marker had among the
                // siblings.
                let (content, content_paths): (Vec<Handle>, Vec<String>) = li_children
                    .iter()
                    .enumerate()
                    .filter_map(|(j, child)| {
                        let path = Self::child_path(&p, child, j);
                        if !checkbox.is_some_and(|(_, input)| Rc::ptr_eq(input, child)) {
                            return Some((child.clone(), path));
                        }
                        ordered_task.then(|| {
                            let checked = Self::attr(child, "checked").is_some();
                            (text_node(&task_marker_text(checked, task_state)), path)
                        })
                    })
                    .unzip();
                items.push(ListItem {
                    attrs: self.attrs(li, &p),
                    checked: (!ordered_task)
                        .then(|| checkbox.map(|(_, input)| Self::attr(input, "checked").is_some()))
                        .flatten(),
                    task_state: if ordered_task { None } else { task_state },
                    children: self.blocks_at(&content, Some(&content_paths), &p, depth + 1)?,
                    pos: None,
                });
            }
            let tight = !list_items.iter().any(|li| self.holds_paragraph(li));
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
                    block_image: false,
                    pos: None,
                })]);
            }
            if let Some(kind) = Self::container_from(&tag, &attrs) {
                let attrs = Self::without_structural_class(attrs, &kind);
                // A Tier-2 container carries a title the same way a callout
                // does, and renders it with no generated id - so the lift has to
                // reach this arm as well, not only the `<aside>` one below.
                //
                // A DIRECTIVE HAS NO TITLE SLOT (CARVE-P12-057 closes the node
                // without one), so nothing is lifted out of a generated-content
                // container: a `<p class="admonition-title">` inside one stays
                // the ordinary paragraph it reads as, rather than being lifted
                // into a field that does not exist and dropped on the way.
                let generated = crate::ast::is_generated_content_kind(&kind);
                let (mut title, body, body_paths) =
                    self.admonition_title(&children, path, depth, true)?;
                let (label, body, body_paths) = self.container_label(body, body_paths, depth)?;
                let mut children = self.blocks_at(&body, Some(&body_paths), path, depth + 1)?;
                self.keep_title_spellable(&mut title, &mut children, path, h);
                if generated {
                    return Ok(vec![BlockNode::Directive(Directive {
                        attrs,
                        kind,
                        title,
                        label,
                        children,
                        pos: None,
                    })]);
                }
                return Ok(vec![BlockNode::Admonition(Admonition {
                    attrs,
                    kind,
                    title,
                    label,
                    children,
                    pos: None,
                })]);
            }
            // THE LABEL IS LIFTED FIRST, because it is half of the test below.
            // `container_label` takes back PART 9 §10's grouping `[label]` from
            // the `<p class="div-label">` the renderer degraded it to, and the
            // `container_from` arm above has had that lift since
            // markup-carve/carve-rs#1310. This arm never got one, so a `<div>`
            // that DID survive still came back with its label as a paragraph -
            // `<div id="foo">` round-tripped to a fence wrapping
            // `{.div-label}` + `g`. The two changes compose here rather than
            // overlap: the widened test below is what lets the fence survive at
            // all, and this is what puts the label back on its opener.
            let body_paths: Vec<String> = children
                .iter()
                .enumerate()
                .map(|(i, child)| Self::child_path(path, child, i))
                .collect();
            let (label, body, body_paths) =
                self.container_label(children.to_vec(), body_paths, depth)?;
            let blocks = self.blocks_at(&body, Some(&body_paths), path, depth + 1)?;
            if attrs.is_none() && label.is_none() {
                return Ok(blocks);
            }
            return Ok(vec![BlockNode::Div(Div {
                attrs,
                label,
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
        if tag == "figure" {
            let carried = attrs.clone();
            // Where the report stood before the rebuild ran. A rebuild
            // `roundtrip` then REJECTS has to leave no trace: every row
            // `figure_panel` and the walk under it pushed describes a tree this
            // mode is about to throw away, and the element it preserves instead
            // loses nothing to report. The node BUDGET stays spent - the
            // subtree was walked, and a figure buys no free walk the rest of
            // the importer does not give.
            let reported = self.diagnostics.len();
            let unspellable = self.unspellable.len();
            let displaced = self.displaced_figure_attrs.len();
            let rebuilt = self.figure_panel_captioned(h, path, depth, attrs, false);
            // A LIMIT REACHED INSIDE THE REBUILD IS NOT THE DOCUMENT'S ERROR
            // in this mode. Before the per-target rule, `roundtrip` preserved a
            // figure WITHOUT walking it, so a body 200 levels deep imported
            // fine as an opaque block; failing the whole document for it now
            // would be a regression bought with the fix. The element that
            // cannot be walked is exactly the element no rebuild can reproduce,
            // so it takes the same exit as one whose target has no spelling.
            //
            // Only `DepthLimit` and `NodeLimit` reach here - the other two
            // variants are raised by the WRITER, after this walk is over - so
            // nothing but a budget is being swallowed. The nodes already
            // counted stay counted: the subtree was walked, and refunding them
            // would sell one budget once per figure.
            let mut detached = false;
            let blocks = match rebuilt {
                // A CAPTION LINE THE TARGET WOULD ABSORB IS NOT WRITTEN IN ANY
                // MODE (ruling markup-carve/carve-php#1731). `roundtrip` keeps
                // the bytes for such a target, below; `safe` and `semantic`
                // cannot preserve, so the rebuilt figure is taken back apart
                // here and the arm at the foot of this block declares what the
                // unwrap cost.
                Ok((blocks, _)) if self.opts.mode != HtmlImportMode::Roundtrip => {
                    let (blocks, declared) = self.unwrap_absorbed_caption(h, path, blocks);
                    // The detach names the `<figcaption>` and what it cost. The
                    // generic row below names the WRAPPER, and both would be
                    // about the same event - so the arm that already spoke says
                    // so rather than letting one shape report twice.
                    detached = declared;
                    blocks
                }
                // A FIGURE IS THE CAPTIONED WRAPPER (PART 9 §4b). An element
                // carrying no `<figcaption>`, or one whose caption spells
                // nothing, never reaches the rebuild-or-preserve decision at
                // all: it is not a figure to rebuild and not one to preserve,
                // so it unwraps to its content with `element-unwrapped` in
                // EVERY mode. `docs/html-import.md` states that as the behavior
                // this rule leaves untouched, and carve-js pins it; without the
                // arm, `roundtrip` preserved every uncaptioned `<figure>` and
                // the two engines disagreed on exactly that shape.
                Ok((blocks, false)) => blocks,
                Ok((blocks, true)) if Self::roundtrip_keeps_rebuilt_figure(h, &blocks) => blocks,
                Err(error) if self.opts.mode != HtmlImportMode::Roundtrip => return Err(error),
                _ => {
                    self.diagnostics.truncate(reported);
                    self.unspellable.truncate(unspellable);
                    self.displaced_figure_attrs.truncate(displaced);
                    self.keep_raw(h, path, &tag);
                    return Ok(vec![BlockNode::RawBlock(RawBlock {
                        format: "html".into(),
                        content: Self::html(h),
                        pos: None,
                    })]);
                }
            };
            match blocks.as_slice() {
                [BlockNode::Figure(f)] => {
                    if matches!(*f.target, FigureTarget::Table(_)) {
                        // THE SENTENCE NO LONGER CLAIMS THE FIGURE'S ATTRIBUTES
                        // SURVIVE, because after the ruling below they may not:
                        // the table writes its OWN attribute line, and where the
                        // two set the same name the table's wins. It said "the
                        // written table carries the caption and the figure's
                        // attributes", which was a false report on exactly the
                        // shape this row is attached to. carve-php recorded that
                        // it could not adopt this wording for that reason
                        // (markup-carve/carve-php#1729); the text is now the one
                        // carve-js and carve-php already share, so one shape
                        // reads the same from all three.
                        self.unspellable.push((
                            h.clone(),
                            path.to_owned(),
                            "A figure wrapping a table has no Carve spelling; the caption is \
                             written on the table, which renders <caption> inside it"
                                .into(),
                            HtmlImportDiagnosticCode::StructureUnspellable,
                        ));
                    }
                    self.record_displaced_figure_attrs(h, path, f);
                }
                // ALREADY DECLARED, and by the row that can say where the
                // text went. The wrapper is gone either way, but its
                // attributes were not lost - they rode onto the table - so the
                // generic pair would report a drop that did not happen.
                _ if detached => {}
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
            let generated = crate::ast::is_generated_content_kind(&kind);
            let (mut title, body, body_paths) =
                self.admonition_title(&children, path, depth, true)?;
            let (label, body, body_paths) = self.container_label(body, body_paths, depth)?;
            let mut children = self.blocks_at(&body, Some(&body_paths), path, depth + 1)?;
            self.keep_title_spellable(&mut title, &mut children, path, h);
            if generated {
                return Ok(vec![BlockNode::Directive(Directive {
                    attrs,
                    kind,
                    title,
                    label,
                    children,
                    pos: None,
                })]);
            }
            return Ok(vec![BlockNode::Admonition(Admonition {
                attrs,
                kind,
                title,
                label,
                children,
                pos: None,
            })]);
        }
        if self.opts.mode == HtmlImportMode::Roundtrip
            && !ROUNDTRIP_UNWRAPPED_SECTIONING.contains(&tag.as_str())
        {
            self.keep_raw(h, path, &tag);
            return Ok(vec![BlockNode::RawBlock(RawBlock {
                format: "html".into(),
                content: Self::html(h),
                pos: None,
            })]);
        }
        let unwrapped = self.is_derived_wrapper(h, &tag)
            || self.report_unsupported_element(h, tag.as_str(), path);
        // THE BODY IS BUILT BEFORE THE ATTRIBUTE ROWS so the wrapper's id has a
        // heading to land on. Whether that id is the renderer's or the author's
        // is a question about the HEADING's own text, and only the imported
        // node carries the inline projection that answers it.
        //
        // This does not reorder the report. Rows sort by the position of the
        // LOSING ELEMENT on the way out, and a wrapper stands before everything
        // it wraps, so moving one call across another only changes the order
        // rows were CONSTRUCTED in - which the sort keeps only as a tie-break
        // between rows at the same position, and these are not.
        let mut blocks = self.blocks(&children, path, depth + 1)?;
        let mut attrs = attrs;
        if self.opts.mode == HtmlImportMode::Roundtrip {
            Self::restore_hoisted_section_id(&tag, &mut attrs, &mut blocks);
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
            if unwrapped {
                "the element was unwrapped and has no node to carry it"
            } else {
                "the empty element was dropped and has no node to carry it"
            },
            path,
        );
        Ok(blocks)
    }

    /// Put a `<section id>` back on the heading the renderer hoisted it off
    /// (markup-carve/carve-rs#1380).
    fn restore_hoisted_section_id(tag: &str, attrs: &mut Option<Attrs>, blocks: &mut [BlockNode]) {
        if tag != "section" {
            return;
        }
        let Some(held) = attrs.as_mut() else {
            return;
        };
        let Some(id) = held.id.clone() else {
            return;
        };
        // The heading the wrapper was built around is its FIRST block. Anything
        // else and the `<section>` is not this renderer's, so its id is not a
        // hoist and stays reported.
        let Some(BlockNode::Heading(heading)) = blocks.first_mut() else {
            return;
        };
        // A heading that already carries an id was never hoisted off - two ids
        // in the rendered document mean two different facts, and overwriting
        // the heading's own with the wrapper's would lose one of them.
        if heading.attrs.as_ref().is_some_and(|held| held.id.is_some()) {
            return;
        }
        held.id = None;
        if Self::is_generated_heading_id(&id, &crate::render::plain_inlines(&heading.children)) {
            return;
        }
        heading.attrs.get_or_insert_with(Attrs::default).id = Some(id);
    }
    /// `<details>/<summary>` to a `details` admonition.
    #[allow(clippy::type_complexity)]
    /// `lift_title` is false for a generated-content container: `directive` has
    /// The first title paragraph becomes the container's title field.
    fn admonition_title(
        &mut self,
        children: &[Handle],
        path: &str,
        depth: usize,
        lift_title: bool,
    ) -> Lifted<Vec<InlineNode>> {
        let at = children
            .iter()
            .position(is_admonition_title)
            .filter(|_| lift_title);
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
        if body[..at]
            .iter()
            .any(|child| !Self::text(child).chars().all(is_layout_space))
        {
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
        // AND EVERYTHING UNDER IT, for the same reason. The lift removes the
        // paragraph before `blocks_at` can reach it, so its text child is a DOM
        // node nothing else will ever charge - and a labelled container would
        // then cost one node and one level LESS than the same DOM without a
        // label, which is a way to process more than `max_nodes` allows by
        // adding markup rather than removing it.
        self.charge_subtree(&body[at], depth + 1)?;
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
        let mut title = title;
        let mut children = self.blocks_at(&body, Some(&body_paths), path, depth + 1)?;
        self.keep_title_spellable(&mut title, &mut children, path, h);
        Ok(Admonition {
            attrs,
            kind: "details".into(),
            title,
            label: None,
            children,
            pos: None,
        })
    }

    /// On the exit that writes source, a title the quoted slot cannot spell
    /// becomes the body's first paragraph. Written as a title it would either
    /// fail the whole write or turn the opener, and the body with it, into a
    /// paragraph (PART 11 section 1c, `structure-unspellable`).
    fn keep_title_spellable(
        &mut self,
        title: &mut Option<Vec<InlineNode>>,
        children: &mut Vec<BlockNode>,
        path: &str,
        h: &Handle,
    ) {
        if !self.writing
            || title
                .as_deref()
                .map_or(true, crate::render_carve::quoted_title_is_spellable)
        {
            return;
        }
        let Some(inlines) = title.take() else {
            return;
        };
        children.insert(
            0,
            BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: inlines,
                at_content_column: true,
                block_image: false,
                pos: None,
            }),
        );
        let tag = Self::tag(h).unwrap_or_default();
        self.diag(
            HtmlImportDiagnosticCode::StructureUnspellable,
            format!(
                "Moved the title of <{tag}> into its body: a quoted title cannot spell a double quote or a line break"
            ),
            HtmlImportSeverity::Warning,
            path,
            h,
        );
    }
    /// The numbering style this `<ol>` is written with, or `None` for decimal.
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
        // Every entry is written, so a `<dl>` imports and writes back as ONE
        // list: no term acquires the next entry's description and nothing
        // splits. See `render_definition_list`.
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
            let children = self.blocks(&node.children.borrow(), &p, node_depth + 1)?;
            // A `<dd>` that writes nothing takes the `{empty}` sentinel, which
            // reads back as a description holding no blocks, so the entry
            // survives the round trip and owes no row (markup-carve/carve#1827).
            definitions.push(DefinitionDef {
                attrs: self.attrs(node, &p),
                children,
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
        // spell (`-2col`, `my.class`) would be written after the colons and read
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
    fn inlines_are_blank(nodes: &[InlineNode]) -> bool {
        nodes.iter().all(|n| match n {
            InlineNode::Text(t) => t.value.chars().all(is_layout_space),
            _ => false,
        })
    }

    /// Report attributes that survived [`Self::attrs`] but have nowhere to go.
    fn has_content_to_unwrap(h: &Handle) -> bool {
        for child in h.children.borrow().iter() {
            match &child.data {
                NodeData::Element { name, .. } => {
                    if !matches!(
                        name.local.as_ref(),
                        "script" | "style" | "template" | "noscript"
                    ) {
                        return true;
                    }
                }
                NodeData::Text { contents } => {
                    if Self::holds_more_than_layout(&contents.borrow()) {
                        return true;
                    }
                }
                NodeData::Comment { contents } if Self::holds_more_than_layout(contents) => {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Whether a run of text is anything other than ASCII layout whitespace.
    fn holds_more_than_layout(text: &str) -> bool {
        text.chars()
            .any(|c| !matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{0c}'))
    }

    /// The row for an element this importer has no mapping for, unwrapped or
    /// dropped by what it carried (markup-carve/carve#1738). Returns whether it
    /// was unwrapped, which is what the attribute rows below it say next.
    ///
    /// EVERY ARM THAT WRITES THE GENERAL ROW, which is what keeps the three
    /// engines answering alike. The row is written from a sectioning wrapper
    /// and from the arm that catches everything with no mapping at all, and the
    /// same tag reaches a different one of them in each engine - a `<form>`
    /// takes this engine's inline arm and carve-js's block arm - so a rule
    /// applied to one arm would have fixed thirty-two shapes and broken seven.
    ///
    /// A `<figure>` is NOT one of them. It has its own rows and its own rulings
    /// (markup-carve/carve#1716, markup-carve/carve#1723), and reaches neither
    /// arm.
    fn report_unsupported_element(&mut self, h: &Handle, tag: &str, path: &str) -> bool {
        if Self::has_content_to_unwrap(h) {
            self.diag(
                HtmlImportDiagnosticCode::ElementUnwrapped,
                format!("Unwrapped unsupported <{tag}> element"),
                HtmlImportSeverity::Info,
                path,
                h,
            );

            return true;
        }
        self.diag(
            HtmlImportDiagnosticCode::ElementDropped,
            format!("Dropped empty <{tag}> element"),
            HtmlImportSeverity::Warning,
            path,
            h,
        );

        false
    }

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
        if !attrs.classes.is_empty() {
            dropped.push(format!("class=\"{}\"", attrs.classes.join(" ")));
        }
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

    /// Whether `roundtrip` keeps what the foreign-`<figure>` rebuild produced,
    /// or throws it away and preserves the element instead
    /// (markup-carve/carve#1704).
    fn roundtrip_keeps_rebuilt_figure(h: &Handle, blocks: &[BlockNode]) -> bool {
        let [BlockNode::Figure(figure)] = blocks else {
            return false;
        };
        let captions_last = h
            .children
            .borrow()
            .iter()
            .filter_map(Self::tag)
            .next_back()
            .is_some_and(|tag| tag == "figcaption");
        if !captions_last {
            return false;
        }
        Self::caption_line_binds(&figure.target)
    }

    /// Does the caption line the rebuild writes BIND to this target, so that it
    /// re-reads as the figure it was written from?
    ///
    /// THE PROPERTY, AND ONE ANSWER FOR EVERY MODE
    /// (markup-carve/carve#1704, ruling markup-carve/carve-php#1731). The modes
    /// differ in what they do with a target the line does not bind to, never in
    /// which targets those are: `roundtrip` keeps the element's bytes, and
    /// `safe` and `semantic`, which cannot preserve, unwrap and declare. Neither
    /// writes the line.
    ///
    /// Image, code block and quote each write a caption line the parser reads
    /// back as the same figure. `Table` is the deliberate carve-out named above:
    /// the line binds to the TABLE, which is why a figure is built for it and
    /// why the writer still has something to declare.
    ///
    /// PROSE ABSORBS THE LINE INSTEAD OF CARRYING IT, which is a different
    /// failure from losing it. `x` then `^ Cap` re-reads as ONE paragraph
    /// holding a literal caret, so the document gains a character its author
    /// never wrote - and an addition cannot be declared away the way a loss can.
    fn caption_line_binds(target: &FigureTarget) -> bool {
        match target {
            FigureTarget::Image(_) | FigureTarget::CodeBlock(_) | FigureTarget::BlockQuote(_) => {
                true
            }
            FigureTarget::Table(table) => match table.caption.as_deref() {
                None => true,
                Some(caption) => Self::inlines_are_blank(caption),
            },
            FigureTarget::Paragraph(_) => false,
        }
    }

    /// A rebuilt figure whose target would ABSORB its caption line, taken back
    /// apart into the blocks the unwrap writes: the body, then the caption as a
    /// paragraph of its own (ruling markup-carve/carve-php#1731).
    ///
    /// The figure is gone either way; what this buys over writing the line
    /// anyway is that no character is invented. `x`, a blank line, then `Cap`
    /// re-reads as two paragraphs - the association is lost, and every byte the
    /// author wrote is still their own. It is the shape carve-php has always
    /// written and the shape the caller declares with `element-unwrapped`.
    ///
    /// THE WRAPPER'S ATTRIBUTES ARE LEFT FOR THE CALLER TO DROP AND REPORT,
    /// rather than landed on the body. An `id` that identified a FIGURE, moved
    /// onto a bare paragraph, identifies something the author never marked; a
    /// declared loss beats a silent substitution.
    ///
    /// Anything else is handed straight back. [`Self::caption_line_binds`] is
    /// the whole test, so a caption target added later inherits this without a
    /// second list to keep in step.
    fn unwrap_absorbed_caption(
        &mut self,
        h: &Handle,
        path: &str,
        blocks: Vec<BlockNode>,
    ) -> (Vec<BlockNode>, bool) {
        let [BlockNode::Figure(figure)] = blocks.as_slice() else {
            return (blocks, false);
        };
        if Self::caption_line_binds(&figure.target) {
            return (blocks, false);
        }
        // Read BEFORE the figure is taken apart, because the merge below is what
        // displaces them and the row has to name the names it displaced.
        let table_collision = matches!(&*figure.target, FigureTarget::Table(_));
        if table_collision {
            self.record_displaced_figure_attrs(h, path, figure);
        }
        let Some(BlockNode::Figure(figure)) = blocks.into_iter().next() else {
            unreachable!("the slice pattern above matched a lone figure")
        };
        let caption = BlockNode::Paragraph(Paragraph {
            attrs: None,
            children: figure.caption,
            at_content_column: true,
            block_image: false,
            pos: None,
        });
        match *figure.target {
            FigureTarget::Paragraph(target) => (vec![BlockNode::Paragraph(target), caption], false),
            FigureTarget::Table(mut table) => {
                // THE FIGURE'S ATTRIBUTES RIDE ONTO THE TABLE
                // (markup-carve/carve#1721). The figure's line is the LEADING
                // one and the table's own wins the names both set, which is the
                // order the two lines already stacked in. Dropping them instead
                // would take an `id` an anchor points at with nothing said.
                if let Some(own) = figure.attrs {
                    crate::parse::merge_leading_attrs(&mut table.attrs, own);
                }
                // ATTACHED TO THE `<figcaption>`, not to the figure. The row's
                // path already names it, and the report sorts by NODE - so
                // hanging it on the figure tied it with the figure's own
                // attribute row and left the two in emission order, which puts
                // the deferred one last. On the caption it sorts after the
                // figure, which is where the reader looks for it.
                let (caption_node, caption_path) = Self::figcaption_site(h, path);
                self.diag(
                    HtmlImportDiagnosticCode::ElementUnwrapped,
                    "Detached a <figcaption> into a paragraph after the table: the table's own \
                     <caption> fills Carve's one caption slot, so the figure's caption keeps its \
                     text and loses its role"
                        .into(),
                    HtmlImportSeverity::Warning,
                    &caption_path,
                    &caption_node,
                );
                (vec![BlockNode::Table(table), caption], true)
            }
            _ => unreachable!("only a paragraph and a self-captioning table refuse the line"),
        }
    }

    /// The `<figcaption>` the detach is about, and its path: the FIRST direct
    /// child of that name, which is the one `figure_panel_captioned` captions
    /// with.
    ///
    /// Falls back to the figure itself, which cannot happen on this arm - a
    /// detach needs a caption to detach - and is spelled rather than
    /// `unwrap`ped so a future caller cannot turn a missing child into a panic.
    fn figcaption_site(h: &Handle, path: &str) -> (Handle, String) {
        let found = h
            .children
            .borrow()
            .iter()
            .enumerate()
            .find(|(_, c)| Self::tag(c).as_deref() == Some("figcaption"))
            .map(|(index, c)| (c.clone(), index));
        match found {
            Some((node, index)) => (node, format!("{path}/figcaption[{}]", index + 1)),
            None => (h.clone(), path.to_owned()),
        }
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
        self.figure_panel_captioned(h, path, depth, attrs, own_output)
            .map(|(blocks, _)| blocks)
    }

    /// [`Self::figure_panel`], and WHETHER THE ELEMENT WAS CAPTIONED AT ALL.
    ///
    /// A figure is the CAPTIONED wrapper (PART 9 §4b), so an element carrying
    /// no `<figcaption>` - or one whose caption spells nothing - is not a
    /// figure to rebuild or to preserve: it unwraps to its content with
    /// `element-unwrapped`, in every mode, which is the behavior the
    /// per-target rule leaves untouched (`docs/html-import.md`,
    /// markup-carve/carve#1704). `roundtrip` has to know that BEFORE it decides
    /// whether to preserve, and only this function knows it: "spells nothing"
    /// is a question about the PARSED inlines, not about the element's text, so
    /// answering it a second time on the DOM would be the same rule written
    /// twice and free to drift.
    fn figure_panel_captioned(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
        own_output: bool,
    ) -> Result<(Vec<BlockNode>, bool), HtmlImportError> {
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
        let is_blank_text = dom_text_is_layout_only;
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
            return Ok((blocks, false));
        };
        if blocks.len() == 1 {
            let target = match blocks.remove(0) {
                BlockNode::Paragraph(p)
                    if p.attrs.is_none()
                        && p.children.len() == 1
                        && matches!(p.children[0], InlineNode::Image(_)) =>
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
                        block_image: false,
                        pos: None,
                    }));
                    return Ok((blocks, true));
                }
            };
            return Ok((
                vec![BlockNode::Figure(Figure {
                    attrs,
                    target: Box::new(target),
                    rendered_target: None,
                    caption,
                    short_caption: None,
                    pos: None,
                })],
                true,
            ));
        }
        blocks.push(BlockNode::Paragraph(Paragraph {
            attrs: None,
            children: caption,
            at_content_column: true,
            block_image: false,
            pos: None,
        }));
        Ok((blocks, true))
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

    /// Drop a body cell's alignment where the HEAD already says it.
    ///
    /// A header cell's marker run is the COLUMN's default: the renderer reads
    /// it off the leading header rows and every cell below inherits what it
    /// does not state. So a body cell repeating its column's value spells a
    /// thing the document already says, and the shortest source that renders
    /// the same table is the one without it - which is also the source a round
    /// trip has to come back to, or `|= h |` over `| a |` grows a marker on
    /// every body row on its first pass through HTML.
    ///
    /// ONLY WHERE THE VALUE AGREES, and only per axis. A body cell that differs
    /// from its column keeps its own run, because that is the only thing that
    /// overrides the default - and a cell agreeing on the horizontal while
    /// stating its own vertical keeps the vertical alone, which is what `?`
    /// exists to spell.
    ///
    /// THE COLUMN WALK IS [`Self::span_grid`]'s, because the renderer resolves
    /// a column by POSITION IN THE ROW'S CELL ARRAY and that array is what
    /// `span_grid` lays out: a carried rowspan mark occupies an index, so the
    /// cells after it shift right by one, and a walk that ignored the marks
    /// would compare a body cell against a column it does not sit in. The mark
    /// bookkeeping is therefore copied rather than re-derived - the marks a row
    /// OPENS do not age at the end of that same row, which is the off-by-one
    /// that would let the row under a rowspan read the wrong column.
    ///
    /// A HEADER CELL SEEDS ITS OWN INDEX ONLY, not the columns a colspan
    /// carries it across, because that is what THIS engine's renderer reads:
    /// the continuation cell at the next index states no alignment and
    /// `table_column_defaults` finds none there. Seeding the span would drop a
    /// body alignment the re-render could not put back, which is the loss this
    /// whole ruling is against.
    fn drop_inherited_cell_alignment(built: &mut [Vec<BuiltCell>], leading_header_rows: usize) {
        if leading_header_rows == 0 {
            return;
        }
        let mut columns: Vec<CellAlignment> = Vec::new();
        let mut carried: BTreeMap<usize, usize> = BTreeMap::new();
        for (r, row) in built.iter_mut().enumerate() {
            let mut opened: Vec<(usize, usize)> = Vec::new();
            let mut column = 0usize;
            for entry in row.iter_mut() {
                // Every index this cell occupies, skipping the ones a rowspan
                // above already holds - the same walk `span_grid` does when it
                // places the continuation marks.
                let mut covered: Vec<usize> = Vec::new();
                for _ in 0..entry.colspan.max(1) {
                    while carried.contains_key(&column) {
                        column += 1;
                    }
                    covered.push(column);
                    column += 1;
                }
                let Some(&own) = covered.first() else {
                    continue;
                };
                if r < leading_header_rows {
                    if columns.len() <= own {
                        columns.resize(own + 1, CellAlignment::default());
                    }
                    // The LAST header row to state an axis wins, which is how
                    // `table_column_defaults` folds several of them.
                    if entry.cell.align.is_some() {
                        columns[own].align = entry.cell.align;
                    }
                    if entry.cell.valign.is_some() {
                        columns[own].valign = entry.cell.valign;
                    }
                } else {
                    let slot = columns.get(own).copied().unwrap_or_default();
                    if entry.cell.align.is_some() && slot.align == entry.cell.align {
                        entry.cell.align = None;
                    }
                    if entry.cell.valign.is_some() && slot.valign == entry.cell.valign {
                        entry.cell.valign = None;
                    }
                }
                if entry.rowspan > 1 {
                    opened.extend(covered.into_iter().map(|index| (index, entry.rowspan - 1)));
                }
            }
            carried = carried
                .into_iter()
                .filter_map(|(index, left)| (left > 1).then_some((index, left - 1)))
                .chain(opened)
                .collect();
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
                colspan: None,
                rowspan: None,
                header,
                span: Some(span),
                align: None,
                valign: None,
                attrs: None,
                children: Vec::new(),
                blocks: None,
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
                            colspan: None,
                            rowspan: None,
                            header: false,
                            span: None,
                            align: None,
                            valign: None,
                            attrs: None,
                            children: Vec::new(),
                            blocks: None,
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
                // THE MARKER RUN IS THE CELL'S OWN FIELD, so a mapped
                // `text-align` / `vertical-align` lands here rather than in the
                // attribute block `attrs` builds (markup-carve/carve#1745,
                // markup-carve/carve#1746).
                let alignment = self.cell_style_alignment(cell);
                row.push(BuiltCell {
                    cell: TableCell {
                        colspan: None,
                        rowspan: None,
                        header: Self::tag(cell).as_deref() == Some("th"),
                        span: None,
                        align: alignment.align,
                        valign: alignment.valign,
                        attrs: self.attrs(cell, &p),
                        children: {
                            self.cell_depth += 1;
                            let children = self.inlines(&cell.children.borrow(), &p, depth + 1);
                            self.cell_depth -= 1;
                            children?
                        },
                        blocks: None,
                        pos: None,
                    },
                    colspan,
                    rowspan,
                });
            }
            built.push(row);
        }
        Self::drop_inherited_cell_alignment(&mut built, leading_header_rows);
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
        // Report section attributes that could not be assigned to the partition.
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
                "thead" => "the head cannot be represented in the retained row partition",
                "tfoot" => "the foot cannot be represented in the retained row partition",
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
        let last_dropped = if self.writing {
            self.drop_blank_rows(&trs, &mut result, path)
        } else {
            None
        };
        let mut caption = match caption_node {
            Some((index, node)) => Some(self.caption_inlines(
                &node,
                &format!("{path}/caption[{}]", index + 1),
                depth,
                "caption",
            )?),
            None => None,
        };
        if let Some(r) = last_dropped.filter(|_| result.is_empty()) {
            // A lone `^ c` line reads as a paragraph, so the caption goes too.
            if caption.take().is_some() {
                self.diag(
                    HtmlImportDiagnosticCode::ElementDropped,
                    "Dropped a caption whose table has no row left".into(),
                    HtmlImportSeverity::Warning,
                    &format!("{path}/tr[{}]", r + 1),
                    &trs[r].0,
                );
            }
        }
        Ok(Table {
            attrs,
            caption,
            short_caption: None,
            columns: Vec::new(),
            rows: result,
            row_groups: row_groups.map(Box::new),
            pos: None,
        })
    }

    /// Drop every row whose cells are all blank, which Carve reads as text
    /// (markup-carve/carve#1954). Returns the source index of the last row
    /// dropped. Writing exit only: the tree renders such a row, and only the
    /// writer has no spelling for it (carve-rs#1735). The writer does not read
    /// `row_groups`, so the partition is left as the HTML stated it.
    fn drop_blank_rows(
        &mut self,
        trs: &[(Handle, Option<usize>)],
        rows: &mut Vec<TableRow>,
        path: &str,
    ) -> Option<usize> {
        let blank = |cell: &TableCell| {
            cell.children.is_empty()
                && cell.span.is_none()
                && cell.align.is_none()
                && cell.valign.is_none()
                && cell.attrs.as_ref().map_or(true, |a| *a == Attrs::default())
        };
        let mut last = None;
        let mut index = 0;
        for (r, (tr, _)) in trs.iter().enumerate() {
            if index >= rows.len() {
                break;
            }
            if !rows[index].cells.iter().all(blank) {
                index += 1;
                continue;
            }
            rows.remove(index);
            self.diag(
                HtmlImportDiagnosticCode::StructureUnspellable,
                "Dropped a row whose every cell is empty: Carve reads such a row as text".into(),
                HtmlImportSeverity::Warning,
                &format!("{path}/tr[{}]", r + 1),
                tr,
            );
            last = Some(r);
        }
        last
    }

    /// `<thead>` / `<tbody>` / `<tfoot>` to `Table::row_groups`, when the
    /// partition says something a reader cannot derive (carve#1210 D1, ruled as
    /// (b); pandoc-carve#61 implements the same rule and carve-js followed).
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

        let mut take_section_attrs = |tag: &str| {
            let matching: Vec<usize> = sections
                .tags
                .iter()
                .enumerate()
                .filter_map(|(i, name)| (name == tag).then_some(i))
                .collect();
            if matching.len() == 1 {
                sections.attrs[matching[0]].take().map(|(attrs, own_path)| {
                    self.unspellable.push((node.clone(), own_path, format!("Dropped rowGroups.{} because Carve source cannot spell section attributes", if tag == "thead" { "headAttrs" } else { "footAttrs" }), HtmlImportDiagnosticCode::StructureUnspellable));
                    attrs
                })
            } else {
                None
            }
        };
        let head_attrs = take_section_attrs("thead");
        let foot_attrs = take_section_attrs("tfoot");
        let body_paths: Vec<Option<String>> = sections
            .attrs
            .iter()
            .map(|slot| slot.as_ref().map(|(_, path)| path.clone()))
            .collect();
        let mut bodies: Vec<TableBodyGroup> = Vec::new();
        let mut body_sections = Vec::new();
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
            body_sections.push(section);
            bodies.push(TableBodyGroup {
                head_rows: group_head,
                body_rows: index - body_start,
                row_head_columns: (row_head_columns > 0).then_some(row_head_columns),
                attrs: own,
            });
        }

        for id in 0..sections.tags.len() {
            if sections.tags[id] != "tbody" || trs.iter().any(|(_, section)| *section == Some(id)) {
                continue;
            }
            let Some((attrs, _)) = sections.attrs[id].take() else {
                continue;
            };
            let index = body_sections
                .iter()
                .position(|section| section.is_some_and(|s| s > id))
                .unwrap_or(bodies.len());
            bodies.insert(
                index,
                TableBodyGroup {
                    head_rows: 0,
                    body_rows: 0,
                    row_head_columns: None,
                    attrs: Some(attrs),
                },
            );
            body_sections.insert(index, Some(id));
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
        if head_attrs.is_none() && head_rows == 0 && bodies.len() == 1 && leading_header_rows > 0 {
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

        for (index, body) in bodies.iter().enumerate() {
            if body.attrs.is_some() {
                let own_path = body_sections[index]
                    .and_then(|id| body_paths[id].clone())
                    .unwrap_or_else(|| path.to_owned());
                self.unspellable.push((node.clone(), own_path, format!("Dropped rowGroups.bodies[{index}].attrs because Carve source cannot spell section attributes"), HtmlImportDiagnosticCode::StructureUnspellable));
            }
        }
        let derivable = head_attrs.is_none()
            && foot_attrs.is_none()
            && head_rows == leading_header_rows
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
            HtmlImportDiagnosticCode::StructureUnspellable,
        ));
        Some(TableRowGroups {
            head_attrs,
            foot_attrs,
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
        let flattening = handles
            .iter()
            .any(|h| Self::tag(h).as_deref().is_some_and(Self::is_block_tag));
        let mut published = false;
        let mut boundary_pending = false;
        for (i, h) in handles.iter().enumerate() {
            let tag = Self::tag(h).unwrap_or_else(|| {
                // A comment names itself, the same way `child_path` spells it:
                // a row about one has to be findable as a comment rather than
                // read as a text node the document does not have.
                if matches!(&h.data, NodeData::Comment { .. }) {
                    "comment()".into()
                } else {
                    "text()".into()
                }
            });
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
        Ok(drop_space_after_hard_break(coalesce(hoist_edge_space(out))))
    }
    /// An HTML comment in an INLINE position, as the delimited Carve comment
    /// (markup-carve/carve#1709).
    fn comment(&mut self, content: &str, path: &str, node: &Handle) -> Vec<InlineNode> {
        let closes_early = content.contains("%}");
        let ends_the_run = content
            .split('\n')
            .skip(1)
            .any(|line| line.chars().all(|c| c == ' ' || c == '\t'));
        if closes_early || ends_the_run {
            let why = if closes_early {
                "holds the comment closer"
            } else {
                "holds a blank line"
            };
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                format!(
                    "Dropped an HTML comment: its text {why}, which ends a Carve inline comment early, and the comment is not moved out of the run to make it spellable"
                ),
                HtmlImportSeverity::Warning,
                path,
                node,
            );
            return Vec::new();
        }
        vec![InlineNode::Comment(Comment {
            block: false,
            delimited: true,
            content: content.to_string(),
            pos: None,
        })]
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
        // AN HTML COMMENT IS A CARVE COMMENT, and this is the INLINE position of
        // it (markup-carve/carve#1709). The block position is `flush_inline_run`.
        //
        // The usual reason this importer drops something is that Carve cannot
        // express the shape. That reason never applied here: Carve HAS comments,
        // so dropping one was a choice to lose bytes the format can hold, in a
        // mode whose whole job is fidelity, made by nobody. A comment renders
        // nothing in either language, so keeping it is invisible in the output
        // and lossless in the source.
        if let NodeData::Comment { contents } = &h.data {
            return Ok(self.comment(contents.as_ref(), path, h));
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
        if self.formula_images.contains(&node_key(h)) {
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                "Dropped <img>: the fallback image of a formula imported as math".into(),
                HtmlImportSeverity::Info,
                path,
                h,
            );
            return Ok(Vec::new());
        }
        if tag == "q" {
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
            if let Some((tier, content)) = self.math_tex(h) {
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
                } else if tier == 3 {
                    self.diag(
                        HtmlImportDiagnosticCode::EncodingAssumed,
                        "Read <math> through its fallback image's alt: nothing declares the encoding of alt, so TeX is assumed".into(),
                        HtmlImportSeverity::Info,
                        path,
                        h,
                    );
                }
                if let Some(image) = fallback_image(h, &self.sibling_index) {
                    if Self::attr(&image, "alt").as_deref().map(str::trim) == Some(content.as_str())
                    {
                        self.formula_images.insert(node_key(&image));
                    }
                }
                return Ok(vec![InlineNode::Math(Math {
                    attrs,
                    display: Self::attr(h, "display").as_deref() == Some("block"),
                    content,
                    label: None,
                    number: None,
                    pos: None,
                })]);
            }
            if self.opts.mode != HtmlImportMode::Roundtrip
                || (self.cell_depth > 0 && Self::html(h).contains(['\r', '\n']))
            {
                self.charge_subtree(h, depth)?;
                if self.opts.mode != HtmlImportMode::Roundtrip {
                    if let Some(text) = linear_math_text(h) {
                        self.diag(
                            HtmlImportDiagnosticCode::ElementUnwrapped,
                            "Imported <math> as its text: no TeX annotation and no alttext, and its tokens read in order".into(),
                            HtmlImportSeverity::Warning,
                            path,
                            h,
                        );
                        return Ok(vec![InlineNode::text(text)]);
                    }
                }
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
        // An empty pair such as `{**}` reads back as text, and an element the
        // HTML leaves empty shows a reader nothing, so it goes without a row.
        // Its attributes can still matter (an `id` is a link target), and an
        // empty span is spellable, so they move onto one.
        if children.is_empty() && is_braced_pair_tag(tag.as_str()) {
            return Ok(attrs
                .map(|attrs| {
                    vec![InlineNode::Span(Span {
                        attrs: Some(attrs),
                        children: Vec::new(),
                        injected: false,
                        pos: None,
                    })]
                })
                .unwrap_or_default());
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
            "code" => {
                let mut value = Self::text(h);
                if self.cell_depth > 0 && value.contains(['\r', '\n']) {
                    self.diag(
                        HtmlImportDiagnosticCode::StructureUnspellable,
                        "Flattened a line break in <code> inside a table cell: a table row is one line".into(),
                        HtmlImportSeverity::Warning,
                        path,
                        h,
                    );
                    value = value.replace("\r\n", " ").replace(['\r', '\n'], " ");
                }
                if !value.is_empty() {
                    return Ok(vec![InlineNode::code(value, attrs)]);
                }
                let pos = Some(candidate_mark(self.empty_code_spans.len()));
                self.empty_code_spans.push((h.clone(), path.to_owned()));
                InlineNode::Code(Code { value, attrs, pos })
            }
            // PART 9 §25's sink would blank this destination, so it is one
            // Carve cannot carry: imported like an empty one, reported as a
            // dropped attribute (markup-carve/carve#2254).
            "a" if Self::attr(h, "href").is_some_and(|v| has_denied_url_scheme(&v)) => {
                self.diag(
                    HtmlImportDiagnosticCode::AttributeDropped,
                    "Dropped href with a denied URL scheme on <a>".into(),
                    HtmlImportSeverity::Warning,
                    path,
                    h,
                );
                return Ok(unwrapped_content(
                    Self::attrs_with_title(h, attrs),
                    children,
                ));
            }
            "img" if Self::attr(h, "src").is_some_and(|v| has_denied_url_scheme(&v)) => {
                self.diag(
                    HtmlImportDiagnosticCode::AttributeDropped,
                    "Dropped src with a denied URL scheme on <img>".into(),
                    HtmlImportSeverity::Warning,
                    path,
                    h,
                );
                return Ok(unwrapped_content(
                    Self::attrs_with_title(h, attrs),
                    image_alt_content(h),
                ));
            }
            "a" if names_no_destination(Self::attr(h, "href").as_deref()) => {
                self.diag(
                    HtmlImportDiagnosticCode::ElementUnwrapped,
                    "Unwrapped <a> with no destination".into(),
                    HtmlImportSeverity::Info,
                    path,
                    h,
                );
                return Ok(unwrapped_content(
                    Self::attrs_with_title(h, attrs),
                    children,
                ));
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
                return Ok(unwrapped_content(
                    Self::attrs_with_title(h, attrs),
                    image_alt_content(h),
                ));
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
                if self.cell_depth > 0 {
                    self.unspellable.push((
                        h.clone(),
                        path.to_owned(),
                        CELL_BREAK_FLATTENED.into(),
                        HtmlImportDiagnosticCode::StructureUnspellable,
                    ));
                }
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
            _ if self.opts.mode == HtmlImportMode::Roundtrip
                && !(self.cell_depth > 0 && Self::html(h).contains(['\r', '\n'])) =>
            {
                self.keep_raw(h, path, &tag);
                InlineNode::RawInline(RawInline {
                    format: "html".into(),
                    content: Self::html(h),
                    injected: false,
                    pos: None,
                })
            }
            _ => {
                let unwrapped = self.report_unsupported_element(h, tag.as_str(), path);
                // The inline twin of the block unwrap above: `<small>`,
                // `<bdi dir="rtl">`, `<bdo>`, `<ruby>`, `<button>`, `<label>`
                // keep their children and nothing else. An element that had no
                // children says so instead, or the report would call the same
                // element dropped in one row and unwrapped in the next.
                self.report_unplaceable_attrs(
                    h,
                    attrs,
                    tag.as_str(),
                    if unwrapped {
                        "the element was unwrapped and has no node to carry it"
                    } else {
                        "the empty element was dropped and has no node to carry it"
                    },
                    path,
                );
                return Ok(children);
            }
        };
        let mut node = node;
        if let InlineNode::Emphasis(Emphasis { pos, .. })
        | InlineNode::CriticInsert(CriticInsert { pos, .. })
        | InlineNode::CriticDelete(CriticDelete { pos, .. }) = &mut node
        {
            *pos = Some(candidate_mark(self.braced_kind_spans.len()));
            self.braced_kind_spans.push((h.clone(), path.to_owned()));
        }
        Ok(vec![node])
    }

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
        // MEASURED (markup-carve/carve-rs#1345): read through `str::trim`, a text
        // node holding one U+00A0 immediately before the separator was chrome, so
        // the walk stepped over it and the content space was gone from both
        // exits. The same slot holding an ordinary word kept it.
        NodeData::Text { .. } => dom_text_is_layout_only(node),
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
        // MEASURED (markup-carve/carve-rs#1345): a `<sup>` holding the backlink
        // and one U+00A0 was read as emptied and detached, taking the content
        // space with it; the same `<sup>` holding a word survived as `{^Z^}`.
        let emptied = parent
            .children
            .borrow()
            .iter()
            .all(|child| match &child.data {
                NodeData::Element { .. } => false,
                NodeData::Text { .. } => dom_text_is_layout_only(child),
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
            // MEASURED (markup-carve/carve-rs#1345): read through `str::trim`, a
            // `<sup>` holding one U+00A0 beside the anchor counted as holding
            // nothing else, so the whole `<sup>` became the reference site and the
            // content space went with it. A word there kept the `<sup>` as
            // `{^Z[^1]^}`.
            NodeData::Text { .. } if !dom_text_is_layout_only(child) => {
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
/// An image's alternative text as the content it unwraps to.
fn image_alt_content(h: &Handle) -> Vec<InlineNode> {
    let alt = Importer::attr(h, "alt").unwrap_or_default();
    if alt.is_empty() {
        Vec::new()
    } else {
        vec![InlineNode::text(alt)]
    }
}

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
/// The elements imported as a node Carve spells as a delimiter pair around its content.
fn is_braced_pair_tag(tag: &str) -> bool {
    matches!(
        tag,
        "em" | "i" | "strong" | "b" | "s" | "strike" | "ins" | "del" | "u" | "mark" | "sub" | "sup"
    )
}

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
fn is_layout_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{000c}')
}

/// Whether a RAW-DOM node is a text node carrying no CONTENT - every character
/// it holds is a layout space, so dropping it drops nothing the author wrote.
///
/// ONE PLACE, because this file kept re-deciding the question at each site, and
/// every site that reached for `str::trim` decided it wrong. `str::trim` and
/// `char::is_whitespace` both read U+00A0, U+202F and U+3000 as whitespace, and
/// markup-carve/carve#1628 rules those three CONTENT - measured, not assumed: a
/// lone U+00A0 line parses to a paragraph where a lone space line is a blank
/// line. So a site that trims is not asking "is this a margin", it is asking
/// "is this whitespace", and for these three characters the two answers differ
/// and the difference is a DELETION.
///
/// Four defects came out of that, each found only by fixing the previous one:
/// markup-carve/carve-rs#1336 (a `<div>` holding one U+00A0 built no paragraph
/// and the document came back empty), markup-carve/carve-rs#1339 (a
/// `<figcaption>` holding one was destroyed), markup-carve/carve-rs#1342 (one
/// inside a `<figure>` deleted from both exits) and markup-carve/carve-rs#1345
/// (the three footnote-detach chrome tests below). A named predicate is what
/// stops the next spelling being written.
fn dom_text_is_layout_only(node: &Handle) -> bool {
    match &node.data {
        NodeData::Text { contents } => contents.borrow().chars().all(is_layout_space),
        _ => false,
    }
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

/// The edge LAYOUT whitespace of a paragraph's run, removed.
fn trim_edge_whitespace(mut nodes: Vec<InlineNode>) -> Vec<InlineNode> {
    while let Some(InlineNode::Text(first)) = nodes.first_mut() {
        first.value = first.value.trim_start_matches(is_layout_space).to_string();
        if first.value.is_empty() {
            nodes.remove(0);
        } else {
            break;
        }
    }
    while let Some(InlineNode::Text(last)) = nodes.last_mut() {
        last.value = last.value.trim_end_matches(is_layout_space).to_string();
        if last.value.is_empty() {
            nodes.pop();
        } else {
            break;
        }
    }
    nodes
}

/// Does this run hold anything a document would keep?
///
/// The synthesized arm's twin of `is_layout_only`, and it has to draw the line
/// in the same place: LAYOUT whitespace builds nothing, and U+00A0, U+202F and
/// U+3000 are CONTENT (markup-carve/carve#1628). This read `str::trim`, whose
/// `char::is_whitespace` covers all three, so `<div>` plus a no-break space
/// built no paragraph at all - the document came back EMPTY, with no diagnostic,
/// for content the ruling had just pinned as content (carve-rs#1336).
fn visible(nodes: &[InlineNode]) -> bool {
    nodes
        .iter()
        .any(|n| !matches!(n, InlineNode::Text(t) if t.value.chars().all(is_layout_space)))
}
/// A line's leading LAYOUT whitespace, dropped after a hard break: the parser
/// drops it from the line the break opens, so writing it breaks `fmt`'s fixed
/// point (markup-carve/carve-rs#1706).
fn drop_space_after_hard_break(mut nodes: Vec<InlineNode>) -> Vec<InlineNode> {
    let mut index = 1;
    while index < nodes.len() {
        if matches!(nodes[index - 1], InlineNode::HardBreak(_)) {
            if let InlineNode::Text(text) = &mut nodes[index] {
                text.value = text.value.trim_start_matches(is_layout_space).to_string();
                if text.value.is_empty() {
                    nodes.remove(index);
                    continue;
                }
            }
        }
        index += 1;
    }
    nodes
}

/// A link's or span's edge whitespace stands outside it, as one space that
/// merges with whitespace already there (markup-carve/carve#2361).
/// Whitespace-only content stays, and so does U+00A0, which is content.
fn hoist_edge_space(nodes: Vec<InlineNode>) -> Vec<InlineNode> {
    fn starts_blank(node: Option<&InlineNode>) -> bool {
        matches!(node, Some(InlineNode::Text(t)) if t.value.starts_with([' ', '\t']))
    }
    fn ends_blank(node: Option<&InlineNode>) -> bool {
        matches!(node, Some(InlineNode::Text(t)) if t.value.ends_with([' ', '\t']))
    }
    fn kids(node: &InlineNode) -> Option<&[InlineNode]> {
        match node {
            InlineNode::Emphasis(e) => Some(&e.children),
            InlineNode::Link(l) => Some(&l.children),
            InlineNode::Span(s) => Some(&s.children),
            _ => None,
        }
    }
    // Through nested inlines: a strong ending in a space already has one there.
    fn deep_starts_blank(node: Option<&InlineNode>) -> bool {
        starts_blank(node)
            || node
                .and_then(kids)
                .is_some_and(|k| deep_starts_blank(k.first()))
    }
    fn deep_ends_blank(node: Option<&InlineNode>) -> bool {
        ends_blank(node)
            || node
                .and_then(kids)
                .is_some_and(|k| deep_ends_blank(k.last()))
    }
    fn hoist(children: &mut Vec<InlineNode>) -> (bool, bool) {
        if is_layout_only(children) {
            return (false, false);
        }
        let lead = starts_blank(children.first());
        if lead {
            if let Some(InlineNode::Text(first)) = children.first_mut() {
                first.value = first.value.trim_start_matches([' ', '\t']).to_string();
                if first.value.is_empty() {
                    children.remove(0);
                }
            }
        }
        let trail = ends_blank(children.last());
        if trail {
            if let Some(InlineNode::Text(last)) = children.last_mut() {
                last.value = last.value.trim_end_matches([' ', '\t']).to_string();
                if last.value.is_empty() {
                    children.pop();
                }
            }
        }
        (lead, trail)
    }
    let mut out: Vec<InlineNode> = Vec::with_capacity(nodes.len());
    let mut owed = false;
    for mut node in nodes {
        let (lead, trail) = match &mut node {
            InlineNode::Link(link) => hoist(&mut link.children),
            InlineNode::Span(span) => hoist(&mut span.children),
            _ => (false, false),
        };
        if (owed || lead)
            && !deep_ends_blank(out.last())
            && !(owed && !lead && deep_starts_blank(Some(&node)))
        {
            out.push(InlineNode::text(" ".to_string()));
        }
        out.push(node);
        owed = trail;
    }
    if owed {
        out.push(InlineNode::text(" ".to_string()));
    }
    out
}

/// A text or comment node that carries nothing a formula shows.
fn is_blank_or_comment(handle: &Handle) -> bool {
    match &handle.data {
        NodeData::Comment { .. } => true,
        NodeData::Text { contents } => contents.borrow().chars().all(is_layout_space),
        _ => false,
    }
}

fn element_name(handle: &Handle) -> Option<String> {
    match &handle.data {
        NodeData::Element { name, .. } => Some(name.local.to_string()),
        _ => None,
    }
}

/// The text of a `<math>` with no TeX, when flattening cannot change its value
/// (markup-carve/carve#2361): tokens in order, inside grouping elements only.
/// A fraction or a script would flatten into a different number, so any other
/// element refuses.
fn linear_math_text(math: &Handle) -> Option<String> {
    fn read(nodes: &[Handle], text: &mut String) -> bool {
        for node in nodes {
            if is_blank_or_comment(node) {
                continue;
            }
            match element_name(node).as_deref() {
                Some("semantics") => {
                    let children = node.children.borrow();
                    let Some(first) = children.iter().find(|c| !is_blank_or_comment(c)) else {
                        return false;
                    };
                    if !read(std::slice::from_ref(first), text) {
                        return false;
                    }
                }
                Some("mrow" | "mstyle" | "mpadded") => {
                    if !read(&node.children.borrow(), text) {
                        return false;
                    }
                }
                Some("mi" | "mn" | "mo" | "mtext") => {
                    let mut token = String::new();
                    for child in node.children.borrow().iter() {
                        let NodeData::Text { contents } = &child.data else {
                            return false;
                        };
                        token.push_str(&contents.borrow());
                    }
                    text.push_str(collapse(&token).trim_matches(' '));
                }
                Some("mspace") => {}
                _ => return false,
            }
        }
        true
    }
    let mut text = String::new();
    (read(&math.children.borrow(), &mut text) && !text.is_empty()).then_some(text)
}

fn hidden_by_style(handle: &Handle) -> bool {
    let NodeData::Element { attrs, .. } = &handle.data else {
        return false;
    };
    let attrs = attrs.borrow();
    let Some(style) = attrs
        .iter()
        .find(|a| a.name.local.as_ref().eq_ignore_ascii_case("style"))
    else {
        return false;
    };
    // The effective value: the last declaration wins, an `!important` one first.
    let mut display: Option<String> = None;
    let mut important = false;
    for declaration in style.value.split(';') {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        if !property.trim().eq_ignore_ascii_case("display") {
            continue;
        }
        let value = value.trim().to_ascii_lowercase();
        let (value, is_important) = match value.strip_suffix("important") {
            Some(rest) if rest.trim_end().ends_with('!') => (
                rest.trim_end().trim_end_matches('!').trim().to_string(),
                true,
            ),
            _ => (value, false),
        };
        if important && !is_important {
            continue;
        }
        display = Some(value);
        important = is_important;
    }
    display.as_deref() == Some("none")
}

/// A `<math>` hidden by `display: none`, on itself or on the `<span>` holding only it.
fn math_is_hidden(math: &Handle) -> bool {
    hidden_by_style(math)
        || parent_handle(math).is_some_and(|wrapper| {
            element_name(&wrapper).as_deref() == Some("span")
                && hidden_by_style(&wrapper)
                && wrapper
                    .children
                    .borrow()
                    .iter()
                    .all(|c| Rc::ptr_eq(c, math) || is_blank_or_comment(c))
        })
}

/// The `<img>` that renders a formula for a reader without MathML: the next
/// element after the `<math>`, or after a `<span>` holding nothing but it.
/// Sibling positions, built once per parent so a run of formulas stays linear.
type SiblingIndex = RefCell<HashMap<usize, HashMap<usize, usize>>>;

fn sibling_position(
    cache: &SiblingIndex,
    parent: &Handle,
    children: &[Handle],
    node: &Handle,
) -> Option<usize> {
    let mut cache = cache.borrow_mut();
    let cached = cache
        .get(&node_key(parent))
        .and_then(|positions| positions.get(&node_key(node)).copied());
    if let Some(index) = cached.filter(|&i| children.get(i).is_some_and(|c| Rc::ptr_eq(c, node))) {
        return Some(index);
    }
    let positions: HashMap<usize, usize> = children
        .iter()
        .enumerate()
        .map(|(i, c)| (node_key(c), i))
        .collect();
    let index = positions.get(&node_key(node)).copied();
    cache.insert(node_key(parent), positions);
    index
}

fn fallback_image(math: &Handle, cache: &SiblingIndex) -> Option<Handle> {
    let next = |node: &Handle| -> Option<Handle> {
        let parent = parent_handle(node)?;
        let children = parent.children.borrow();
        let index = sibling_position(cache, &parent, &children, node)?;
        children[index + 1..]
            .iter()
            .find(|c| !is_blank_or_comment(c))
            .cloned()
    };
    let found = match next(math) {
        Some(found) => Some(found),
        None => parent_handle(math)
            .filter(|wrapper| {
                element_name(wrapper).as_deref() == Some("span")
                    && wrapper
                        .children
                        .borrow()
                        .iter()
                        .all(|c| Rc::ptr_eq(c, math) || is_blank_or_comment(c))
            })
            .and_then(|wrapper| next(&wrapper)),
    };
    found.filter(|f| element_name(f).as_deref() == Some("img"))
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

/// The block a SYNTHESIZED wrapper becomes: the image itself when that is all
/// the run holds, and a paragraph otherwise (PART 9 section 4b,
/// markup-carve/carve-rs#1334).
fn synthesized_wrapper(children: Vec<InlineNode>) -> BlockNode {
    match <[InlineNode; 1]>::try_from(children) {
        Ok([InlineNode::Image(image)]) => BlockNode::BlockImage(image),
        Ok([only]) => BlockNode::Paragraph(Paragraph {
            attrs: None,
            children: vec![only],
            at_content_column: true,
            block_image: false,
            pos: None,
        }),
        Err(children) => BlockNode::Paragraph(Paragraph {
            attrs: None,
            children,
            at_content_column: true,
            block_image: false,
            pos: None,
        }),
    }
}

/// The one image a paragraph's run holds, when it holds nothing else.
///
/// THE RUN ARRIVES TRIMMED, so this asks the plain question and nothing more.
/// It used to carry a tolerance for whitespace-only text nodes, which is what
/// made `<p>` and a newline and `  <img>` reach the row - and carve-rs#1336
/// moved that job to `trim_edge_whitespace`, which now runs on the authored arm
/// as well. After that the tolerance could not be reached: a whitespace-only
/// text node surviving the trim would have to be INTERIOR, and an interior node
/// needs a non-text node on each side, which is a second one this returns `None`
/// for. Removing the arm left all sixty import, image and whitespace test
/// binaries green, so it was a branch no input could take - and worse than
/// merely dead, it told a reader the padded spelling was handled HERE when the
/// trim above is what handles it.
fn lone_image(inlines: &[InlineNode]) -> Option<&Image> {
    match inlines {
        [InlineNode::Image(image)] => Some(image),
        _ => None,
    }
}

/// A `<p>` the AUTHOR wrote holding nothing but an image.
///
/// Only what the ROW needs: where it was, whether it had attributes of its own
/// to re-attach, and which of those the image overwrites outright.
struct LoneImageParagraph {
    node: Handle,
    path: String,
    /// Whether the paragraph carried attributes at all.
    attributed: bool,
    /// The names the image's own attribute block wins outright, so they are lost.
    overwritten: Vec<String>,
}

/// The mark a candidate paragraph carries until the survivor scan takes it off.
///
/// WHY A MARK AND NOT A COMPARISON. Two `<p><img src="a" alt="a"></p>` in one
/// document build paragraphs that are EQUAL as values, so matching a candidate
/// against the finished tree by equality cannot say which of them survived - and
/// a bare `<img>` builds a lone-image paragraph of its own here, so the tree
/// holds shapes no `<p>` ever produced. A mark carries identity through the move
/// into the tree, which is what carve-js gets from object identity.
///
/// `pos` IS FREE TO CARRY IT: every construction site in this importer sets
/// `pos: None`, so a `Some` here is this scan's and nothing else's. It is taken
/// back off by the same walk that reads it, on BOTH exits, before either returns
/// - the tree an `html_to_ast` caller receives never carries one.
const EMPTY_CODE_DROPPED: &str = "Dropped an empty <code>: its backtick run is closed by the end of a block or by a forced span, and here the run would read what follows it as code instead";
const CELL_BREAK_FLATTENED: &str = "Wrote a <br> in a table cell as a space: a cell is one line, so no Carve spelling keeps the break";
const EMPTY_WRAPPER_DROPPED: &str = "Dropped an inline element that held only a dropped empty <code>: an empty pair of its delimiters reads back as text";
const EMPTY_CODE_ATTRIBUTES_DROPPED: &str = "Dropped the attributes of an empty <code>: an attribute block attaches to a closing backtick run, which an empty span has not got";
/// A checkbox read on an ordered list item, kept as the item's bracket text.
///
/// Shared with the Markdown entry point, because it is one loss with one cause
/// and a consumer filtering on the message should not have to know which
/// importer ran. The grammar clause behind it stays in the import contract's
/// prose rather than in the row (carve-js#2062).
pub(crate) const ORDERED_TASK_ITEM_UNSPELLABLE: &str = "An ordered task item is not spellable as a Carve task item; the checkbox marker was kept as text";

/// The bracket pair the Carve writer spells behind a bullet (PART 11 §6g), for
/// the ordered item where it can only be text.
fn task_marker_text(checked: bool, state: Option<char>) -> String {
    let state = state.unwrap_or(if checked { 'x' } else { ' ' });
    format!("[{state}]")
}

/// A text node standing where an element the writer cannot spell stood.
fn text_node(text: &str) -> Handle {
    Node::new(NodeData::Text {
        contents: RefCell::new(text.into()),
    })
}

/// Walk one inline sequence as the Carve writer reads an empty code span's run,
/// letting `keep` decide each empty span; `ends` is whether its run ends there.
fn settle_empty_code_spans(
    nodes: &mut Vec<InlineNode>,
    followed: bool,
    labelled: bool,
    cell_not_last: bool,
    writing: bool,
    keep: &mut impl FnMut(&mut Code, bool) -> bool,
    emptied: &mut impl FnMut(),
) {
    let mut index = 0;
    while index < nodes.len() {
        let after = followed || crate::render_carve::run_reads_on(&nodes[index + 1..]);
        if let InlineNode::Code(code) = &mut nodes[index] {
            if code.value.is_empty() {
                // Trailing whitespace is trimmed rather than read into the run,
                // as carve-php's importer trims it.
                let blank_rest = nodes[index + 1..].iter().all(
                    |next| matches!(next, InlineNode::Text(text) if text.value.trim().is_empty()),
                );
                let ends = crate::render_carve::empty_code_position_ends_its_run(
                    followed || (after && !blank_rest),
                    labelled,
                    cell_not_last,
                );
                let InlineNode::Code(code) = &mut nodes[index] else {
                    unreachable!();
                };
                if !keep(code, ends) {
                    nodes.remove(index);
                    merge_text_at(nodes, index);
                    continue;
                }
                if ends && writing {
                    nodes.truncate(index + 1);
                }
            }
        } else if let Some((braced, children)) =
            crate::render_carve::empty_code_run_children_mut(&mut nodes[index])
        {
            let held = !children.is_empty();
            if braced {
                settle_empty_code_spans(
                    children,
                    false,
                    labelled,
                    cell_not_last,
                    writing,
                    keep,
                    emptied,
                );
            } else {
                settle_empty_code_spans(
                    children,
                    after,
                    true,
                    cell_not_last,
                    writing,
                    keep,
                    emptied,
                );
            }
            // An emptied pair such as `{**}` reads back as its delimiters.
            if braced && held && children.is_empty() {
                nodes.remove(index);
                merge_text_at(nodes, index);
                emptied();
                continue;
            }
        }
        index += 1;
    }
}

const NESTED_SAME_KIND_UNWRAPPED: &str = "Unwrapped an inline element nested in one of its own kind: a span inside a span of the same kind has no Carve spelling, so its content is written in the outer one";

/// Unwrap each inner span of a same-kind nesting the writer refuses (ruling
/// markup-carve/carve#2066), one render at a time until none is left.
fn unwrap_nested_same_kind_spans(document: &mut Document, importer: &mut Importer) {
    let mut nested = false;
    let mut probe = |nodes: &mut Vec<InlineNode>, _: bool| {
        nested |= holds_same_kind_nesting(nodes, &mut Vec::new());
    };
    for blocks in document.footnote_defs.values_mut() {
        for_each_inline_run(blocks, &mut probe);
    }
    for_each_inline_run(&mut document.children, &mut probe);
    if !nested || render_carve(document).is_ok() {
        return;
    }
    let refused: HashSet<usize> = crate::render_carve_error::take_nested_same_kind()
        .into_iter()
        .collect();
    let mut unwrapped = Vec::new();
    let mut unwrap = |nodes: &mut Vec<InlineNode>, _: bool| {
        take_span_marks(nodes, &mut |mark| {
            let hit = refused.contains(&mark);
            if hit {
                unwrapped.push(mark - 1);
            }
            hit
        })
    };
    for blocks in document.footnote_defs.values_mut() {
        for_each_inline_run(blocks, &mut unwrap);
    }
    for_each_inline_run(&mut document.children, &mut unwrap);
    unwrapped.sort_unstable();
    for index in unwrapped {
        let (node, path) = importer.braced_kind_spans[index].clone();
        importer.diag(
            HtmlImportDiagnosticCode::StructureUnspellable,
            NESTED_SAME_KIND_UNWRAPPED.into(),
            HtmlImportSeverity::Warning,
            &path,
            &node,
        );
    }
}

/// Whether a span sits anywhere inside a span of its own kind.
fn holds_same_kind_nesting(nodes: &mut [InlineNode], open: &mut Vec<SpanKind>) -> bool {
    for node in nodes {
        let kind = match node {
            InlineNode::Emphasis(emphasis) => Some(SpanKind::Emphasis(emphasis.kind)),
            InlineNode::CriticInsert(_) => Some(SpanKind::Insert),
            InlineNode::CriticDelete(_) => Some(SpanKind::Delete),
            _ => None,
        };
        if let Some(kind) = kind {
            if open.contains(&kind) {
                return true;
            }
            open.push(kind);
        }
        let found = crate::render_carve::empty_code_run_children_mut(node)
            .is_some_and(|(_, children)| holds_same_kind_nesting(children, open));
        if kind.is_some() {
            open.pop();
        }
        if found {
            return true;
        }
    }
    false
}

#[derive(Clone, Copy, PartialEq)]
enum SpanKind {
    Emphasis(EmphasisKind),
    Insert,
    Delete,
}

fn strip_span_marks(nodes: &mut [InlineNode]) {
    for node in nodes {
        if let InlineNode::Emphasis(Emphasis { pos, .. })
        | InlineNode::CriticInsert(CriticInsert { pos, .. })
        | InlineNode::CriticDelete(CriticDelete { pos, .. }) = node
        {
            *pos = None;
        }
        if let Some((_, children)) = crate::render_carve::empty_code_run_children_mut(node) {
            strip_span_marks(children);
        }
    }
}

/// Replace each marked span under `nodes` that `unwrap` names with its children.
fn take_span_marks(nodes: &mut Vec<InlineNode>, unwrap: &mut impl FnMut(usize) -> bool) {
    let mut index = 0;
    while index < nodes.len() {
        if let Some((_, children)) =
            crate::render_carve::empty_code_run_children_mut(&mut nodes[index])
        {
            take_span_marks(children, unwrap);
        }
        let mark = match &mut nodes[index] {
            InlineNode::Emphasis(Emphasis { pos, .. })
            | InlineNode::CriticInsert(CriticInsert { pos, .. })
            | InlineNode::CriticDelete(CriticDelete { pos, .. }) => {
                pos.as_ref().map(|pos| pos.start_offset)
            }
            _ => None,
        };
        if let Some(mark) = mark {
            if unwrap(mark) {
                let children = match nodes.remove(index) {
                    InlineNode::Emphasis(e) => e.children,
                    InlineNode::CriticInsert(i) => i.children,
                    InlineNode::CriticDelete(d) => d.children,
                    _ => unreachable!(),
                };
                let count = children.len();
                nodes.splice(index..index, children);
                merge_text_at(nodes, index + count);
                merge_text_at(nodes, index);
                index = index.saturating_sub(1);
                continue;
            }
        }
        index += 1;
    }
}

/// Join the text runs a removal at `index` left side by side.
fn merge_text_at(nodes: &mut Vec<InlineNode>, index: usize) {
    if index == 0 || index >= nodes.len() {
        return;
    }
    if let (InlineNode::Text(_), InlineNode::Text(_)) = (&nodes[index - 1], &nodes[index]) {
        let InlineNode::Text(next) = nodes.remove(index) else {
            unreachable!();
        };
        if let InlineNode::Text(previous) = &mut nodes[index - 1] {
            previous.value.push_str(&next.value);
        }
    }
}

/// Every inline sequence under `blocks`, with whether it is a table cell that
/// is not the last in its row. EVERY VARIANT, WITH NO WILDCARD ARM, for the
/// reason `take_candidate_marks` gives.
fn for_each_inline_run(blocks: &mut [BlockNode], f: &mut impl FnMut(&mut Vec<InlineNode>, bool)) {
    fn table(table: &mut Table, f: &mut impl FnMut(&mut Vec<InlineNode>, bool)) {
        if let Some(caption) = &mut table.caption {
            f(caption, false);
        }
        if let Some(short) = &mut table.short_caption {
            f(short, false);
        }
        for row in &mut table.rows {
            let cells = row.cells.len();
            for (index, cell) in row.cells.iter_mut().enumerate() {
                f(&mut cell.children, index + 1 < cells);
            }
        }
    }
    for block in blocks {
        match block {
            BlockNode::Heading(n) => f(&mut n.children, false),
            BlockNode::CitationDefinition(n) => f(&mut n.children, false),
            BlockNode::Paragraph(n) => f(&mut n.children, false),
            BlockNode::List(n) => {
                for item in &mut n.items {
                    for_each_inline_run(&mut item.children, f);
                }
            }
            BlockNode::BlockQuote(n) => for_each_inline_run(&mut n.children, f),
            BlockNode::Table(n) => table(n, f),
            BlockNode::Admonition(n) => {
                if let Some(title) = &mut n.title {
                    f(title, false);
                }
                for_each_inline_run(&mut n.children, f);
            }
            BlockNode::Directive(n) => {
                if let Some(title) = &mut n.title {
                    f(title, false);
                }
                for_each_inline_run(&mut n.children, f);
            }
            BlockNode::Div(n) => for_each_inline_run(&mut n.children, f),
            BlockNode::Section(n) => for_each_inline_run(&mut n.children, f),
            BlockNode::LineBlock(n) => for_each_inline_run(&mut n.children, f),
            BlockNode::DefinitionList(n) => {
                for item in &mut n.items {
                    for term in &mut item.terms {
                        f(&mut term.children, false);
                    }
                    for definition in &mut item.definitions {
                        for_each_inline_run(&mut definition.children, f);
                    }
                }
            }
            BlockNode::Figure(n) => {
                match n.target.as_mut() {
                    FigureTarget::Paragraph(paragraph) => f(&mut paragraph.children, false),
                    FigureTarget::BlockQuote(quote) => for_each_inline_run(&mut quote.children, f),
                    FigureTarget::Table(t) => table(t, f),
                    FigureTarget::CodeBlock(_) | FigureTarget::Image(_) => {}
                }
                f(&mut n.caption, false);
                if let Some(short) = &mut n.short_caption {
                    f(short, false);
                }
            }
            BlockNode::FigureGroup(n) => {
                for_each_inline_run(&mut n.children, f);
                if let Some(caption) = &mut n.caption {
                    f(caption, false);
                }
            }
            BlockNode::BlockExtension(n) => for_each_inline_run(n.fallback_slice_mut(), f),
            BlockNode::ExtensionCarrier(n) => {
                if let Some(summary) = &mut n.summary {
                    f(summary, false);
                }
                for_each_inline_run(&mut n.children, f);
            }
            BlockNode::CodeBlock(_)
            | BlockNode::AbbreviationDef(_)
            | BlockNode::LinkReferenceDefinition(_)
            | BlockNode::RawBlock(_)
            | BlockNode::Comment(_)
            | BlockNode::BlockImage(_)
            | BlockNode::ThematicBreak(_) => {}
        }
    }
}

fn candidate_mark(index: usize) -> Pos {
    Pos {
        start_offset: index + 1,
        ..Pos::default()
    }
}

/// Take every candidate mark off the tree, recording which ones were still on it.
///
/// EVERY VARIANT, WITH NO WILDCARD ARM. A variant added to `BlockNode` breaks
/// this build instead of silently becoming a place a surviving paragraph is
/// never found - and a paragraph the walk misses reads as "an unwrapper took it
/// off", which drops a row that was owed and leaves a mark on a tree that is
/// about to be handed to a caller. Both failures are silent, which is why the
/// compiler is made to catch them.
fn take_candidate_marks(blocks: &mut [BlockNode], kept: &mut [bool]) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => take_candidate_mark(paragraph, kept),
            BlockNode::List(list) => {
                for item in &mut list.items {
                    take_candidate_marks(&mut item.children, kept);
                }
            }
            BlockNode::BlockQuote(quote) => take_candidate_marks(&mut quote.children, kept),
            BlockNode::Admonition(admonition) => {
                take_candidate_marks(&mut admonition.children, kept)
            }
            BlockNode::Directive(div) => take_candidate_marks(&mut div.children, kept),
            BlockNode::Div(div) => take_candidate_marks(&mut div.children, kept),
            BlockNode::Section(div) => take_candidate_marks(&mut div.children, kept),
            BlockNode::LineBlock(line_block) => {
                take_candidate_marks(&mut line_block.children, kept)
            }
            BlockNode::DefinitionList(list) => {
                for item in &mut list.items {
                    for definition in &mut item.definitions {
                        take_candidate_marks(&mut definition.children, kept);
                    }
                }
            }
            BlockNode::Figure(figure) => match figure.target.as_mut() {
                FigureTarget::Paragraph(paragraph) => take_candidate_mark(paragraph, kept),
                FigureTarget::BlockQuote(quote) => take_candidate_marks(&mut quote.children, kept),
                // Hold no blocks: a table cell keeps inlines, and the other two
                // are leaves.
                FigureTarget::Table(_) | FigureTarget::CodeBlock(_) | FigureTarget::Image(_) => {}
            },
            BlockNode::FigureGroup(group) => take_candidate_marks(&mut group.children, kept),
            BlockNode::BlockExtension(n) => take_candidate_marks(n.fallback_slice_mut(), kept),
            BlockNode::ExtensionCarrier(extension) => {
                take_candidate_marks(&mut extension.children, kept)
            }
            // A table cell holds INLINES, so no paragraph is ever built inside
            // one - which is also why a `<td><p><img></p></td>` owes no row.
            BlockNode::Table(_)
            | BlockNode::Heading(_)
            | BlockNode::CodeBlock(_)
            | BlockNode::AbbreviationDef(_)
            | BlockNode::LinkReferenceDefinition(_)
            | BlockNode::CitationDefinition(_)
            | BlockNode::RawBlock(_)
            | BlockNode::Comment(_)
            | BlockNode::BlockImage(_)
            | BlockNode::ThematicBreak(_) => {}
        }
    }
}

fn take_candidate_mark(paragraph: &mut Paragraph, kept: &mut [bool]) {
    let Some(ref pos) = paragraph.pos else {
        return;
    };
    let Some(index) = pos.start_offset.checked_sub(1) else {
        return;
    };
    if index < kept.len() {
        kept[index] = true;
        paragraph.pos = None;
    }
}

/// The paragraph attribute names an image's OWN attribute block overwrites.
///
/// The writer emits the paragraph's attributes as a block above the image and
/// the image's inline `{...}` after it, and the two are then read onto one node:
/// a name the image also sets is the one that survives. CLASSES ARE NOT IN THIS
/// SET - the class slot merges rather than replacing, so both groups reach the
/// rendered element and nothing is lost.
///
/// An image's `title` is not here either, and for a different reason: it is a
/// field of its own that the writer puts in the DESTINATION's title slot rather
/// than in the attribute block, so it never collides with a `title=` the
/// paragraph carried.
fn overwritten_attr_names(paragraph: Option<&Attrs>, image: Option<&Attrs>) -> Vec<String> {
    let (Some(paragraph), Some(image)) = (paragraph, image) else {
        return Vec::new();
    };
    let mut lost = Vec::new();
    if paragraph.id.is_some() && image.id.is_some() {
        lost.push("id".to_string());
    }
    for key in paragraph.key_values.keys() {
        if image.key_values.contains_key(key) {
            lost.push(key.clone());
        }
    }
    lost.sort();
    lost
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
        writing,
        diagnostics: Vec::new(),
        turned_away: None,
        document_order: HashMap::new(),
        nodes: 0,
        quote_depth: 0,
        cell_depth: 0,
        unspellable: Vec::new(),
        displaced_figure_attrs: Vec::new(),
        lone_image_paragraphs: Vec::new(),
        empty_code_spans: Vec::new(),
        braced_kind_spans: Vec::new(),
        footnote_refs: HashMap::new(),
        formula_images: HashSet::new(),
        sibling_index: RefCell::new(HashMap::new()),
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
    let mut footnote_defs = importer.adapter_footnotes(
        &dom.document,
        matches!(
            options.adapter,
            HtmlImportAdapter::Word | HtmlImportAdapter::GoogleDocs
        ),
    )?;
    let mut children = importer.blocks(&fragment_top_level(&dom.document), "", 0)?;
    // SURVIVORS ONLY, and the marks come off on BOTH exits. A candidate whose
    // paragraph an unwrapper took back off is not a loss: `caption_host` gives
    // the figure the IMAGE as its target, so both exits keep the same node and a
    // row there would declare a difference that is not there. The scan runs
    // whether or not this exit writes source, because a mark left on the tree
    // would be a position the parser never produced.
    let mut kept = vec![false; importer.lone_image_paragraphs.len()];
    for blocks in footnote_defs.values_mut() {
        take_candidate_marks(blocks, &mut kept);
    }
    take_candidate_marks(&mut children, &mut kept);
    let empty_code_spans = std::mem::take(&mut importer.empty_code_spans);
    let last_dropped = std::cell::Cell::new(None);
    let emptied_wrappers = std::cell::RefCell::new(Vec::new());
    let mut settle = |nodes: &mut Vec<InlineNode>, cell_not_last: bool| {
        settle_empty_code_spans(
            nodes,
            false,
            false,
            cell_not_last,
            writing,
            &mut |code, ends| {
                let Some(index) = code.pos.take().map(|pos| pos.start_offset - 1) else {
                    return true;
                };
                if !writing {
                    return true;
                }
                let (node, path) = &empty_code_spans[index];
                if !ends {
                    last_dropped.set(Some(index));
                    importer.diag(
                        HtmlImportDiagnosticCode::StructureUnspellable,
                        EMPTY_CODE_DROPPED.into(),
                        HtmlImportSeverity::Warning,
                        path,
                        node,
                    );
                    return false;
                }
                if code.attrs.take().is_some() {
                    importer.diag(
                        HtmlImportDiagnosticCode::AttributeDropped,
                        EMPTY_CODE_ATTRIBUTES_DROPPED.into(),
                        HtmlImportSeverity::Info,
                        path,
                        node,
                    );
                }
                true
            },
            &mut || {
                if let Some(index) = last_dropped.get() {
                    emptied_wrappers.borrow_mut().push(index);
                }
            },
        );
    };
    for blocks in footnote_defs.values_mut() {
        for_each_inline_run(blocks, &mut settle);
    }
    for_each_inline_run(&mut children, &mut settle);
    let mut depth_above: HashMap<usize, usize> = HashMap::new();
    for index in emptied_wrappers.into_inner() {
        let (node, path) = &empty_code_spans[index];
        let depth = depth_above.entry(index).or_insert(0);
        *depth += 1;
        let wrapper = path.rsplitn(*depth + 1, '/').last().unwrap_or(path);
        importer.diag(
            HtmlImportDiagnosticCode::StructureUnspellable,
            EMPTY_WRAPPER_DROPPED.into(),
            HtmlImportSeverity::Warning,
            wrapper,
            node,
        );
    }
    if writing {
        for (node, path, message, code) in std::mem::take(&mut importer.unspellable) {
            importer.diag(code, message, HtmlImportSeverity::Warning, &path, &node);
        }
        // ONE ROW PER DISPLACED NAME, at `Info`, which is what this code means
        // everywhere else: an attribute the output does not carry. The figure's
        // target now gets its own attribute line written (`render_figure`), and
        // this is the other half of that ruling - the side that loses is
        // DECLARED rather than resolved in silence (markup-carve/carve#1721).
        for (node, path, name) in std::mem::take(&mut importer.displaced_figure_attrs) {
            importer.diag(
                HtmlImportDiagnosticCode::AttributeDropped,
                format!(
                    "Dropped one {name} on <figure>: the figure and its target both set {name}, \
                     and their two attribute lines merge into a single value"
                ),
                HtmlImportSeverity::Info,
                &path,
                &node,
            );
        }
        for (candidate, kept) in std::mem::take(&mut importer.lone_image_paragraphs)
            .into_iter()
            .zip(kept)
        {
            if !kept {
                continue;
            }
            let head = "A paragraph holding nothing but an image has no Carve spelling; \
the image is written as a block";
            // THREE OUTCOMES, AND THE MESSAGE SAYS WHICH ONE HAPPENED. The plain
            // one loses the `<p>` and nothing else. An attributed one re-attaches
            // what the paragraph carried to the image, which is a different
            // element to carry it. And where the image sets the SAME name, the
            // image's own value wins and the paragraph's is gone -
            // `<p id="p"><img id="i">` writes `{#p}` above `![a](a){#i}` and
            // reads back with `id="i"` alone, so a message claiming the
            // attributes were written on the image would leave that loss
            // undeclared, which is the defect this row exists for.
            let message = if !candidate.attributed {
                format!("{head}, which renders without the <p> around it")
            } else if candidate.overwritten.is_empty() {
                format!(
                    "{head}, so the <p> is lost and the attributes it carried are written on the image instead"
                )
            } else {
                format!(
                    "{head}, so the <p> is lost and the attributes it carried are written on the image - except {}, which the image's own value overwrites",
                    candidate.overwritten.join(", ")
                )
            };
            importer.diag(
                HtmlImportDiagnosticCode::StructureUnspellable,
                message,
                HtmlImportSeverity::Warning,
                &candidate.path,
                &candidate.node,
            );
        }
    }
    let mut document = Document {
        frontmatter: BTreeMap::new(),
        frontmatter_raw: None,
        footnote_defs,
        footnote_def_pos: BTreeMap::new(),
        children,
        source_len: 0,
        ingest_payload_len: 0,
    };
    if writing {
        unwrap_nested_same_kind_spans(&mut document, &mut importer);
    }
    for blocks in document.footnote_defs.values_mut() {
        for_each_inline_run(blocks, &mut |nodes, _| strip_span_marks(nodes));
    }
    for_each_inline_run(&mut document.children, &mut |nodes, _| {
        strip_span_marks(nodes)
    });
    Ok(HtmlImportResult {
        value: document,
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
