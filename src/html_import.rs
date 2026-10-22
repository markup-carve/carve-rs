//! HTML5-to-Carve migration boundary.
mod footnotes;
mod inlines;
mod tables;
use footnotes::{
    has_class, node_key, parent_handle, valid_footnote_carrier_label, AdmonitionTitleIndex,
    FOOTNOTE_PLACEMENT_TAG,
};
mod blocks;
mod figures;

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
    /// Write a table whose cells hold blocks as `::: list-table` rather than
    /// flattening its cells (docs/html-import-contract.md, "A table whose cells
    /// hold blocks can be written as a list table"). Off by default: ListTable
    /// is a Tier-2 extension the reading processor has to enable.
    pub list_table_for_block_cells: bool,
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
            list_table_for_block_cells: false,
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

type IndexBackrefEntry = (String, HashMap<usize, usize>);

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
    carrier_definitions: Vec<BlockNode>,
    reference_definition_keys: HashMap<String, usize>,
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
    code_language_wrappers: HashMap<usize, Option<usize>>,
    nodes: usize,
    /// How many `<q>` elements are open around the node being read. HTML5
    /// leaves the marks to the user agent and every one of them alternates, so
    /// the depth is what chooses between the double and the single pair.
    quote_depth: usize,
    /// How many table cells the walk is inside.
    cell_depth: usize,
    heading_depth: usize,
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
    footnote_refs: HashMap<usize, (Handle, Footnote)>,
    detached_footnote_blocks: HashSet<usize>,
    admonition_title_ids: RefCell<Option<AdmonitionTitleIndex>>,
    /// Fallback images of formulas that imported as math, dropped where they stand.
    formula_images: HashSet<usize>,
    sibling_index: SiblingIndex,
    index_backref_entries: RefCell<HashMap<usize, IndexBackrefEntry>>,
}

/// Where the walk's reportable state stood before a measurement that rewinds.
/// See [`Importer::mark`].
#[derive(Clone, Copy)]
struct ImporterMark {
    carrier_definitions: usize,
    nodes: usize,
    diagnostics: usize,
    turned_away: Option<(usize, usize)>,
    unspellable: usize,
    displaced_figure_attrs: usize,
    lone_image_paragraphs: usize,
    empty_code_spans: usize,
    braced_kind_spans: usize,
}

/// The largest position a Roman marker is written for: `MMMCMXCIX`, the end of
/// the classic range. Past it the additive form grows without bound - the
/// marker for position `n` carries `n / 1000` copies of `m` - and `start` is an
/// author-supplied integer, so the cap is what stops a twenty-byte attribute
/// from buying an arbitrarily large marker once per item.
const MAX_ROMAN_MARKER: usize = 3999;

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
                match Self::refusal(node, tag, &name, &value, &style_filled, self.opts.mode) {
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
                // `FootnotePruner` leaves where a non-final endnotes
                // section sat. It belongs here because it STANDS WHERE A
                // `<section>` STOOD, and a name this list does not recognize is
                // buffered as INLINE - which put the placement inside the
                // paragraph after it rather than between the two.
                | FOOTNOTE_PLACEMENT_TAG
        )
    }

    fn code_language_wrapper(&mut self, parent: &Handle, child: &Handle) -> bool {
        if Self::tag(parent).as_deref() != Some("div") {
            return false;
        }
        let element = self
            .code_language_wrappers
            .entry(Rc::as_ptr(parent) as usize)
            .or_insert_with(|| {
                let mut element = None;
                for node in parent.children.borrow().iter() {
                    match &node.data {
                        NodeData::Element { .. } if element.is_none() => {
                            element = Some(Rc::as_ptr(node) as usize)
                        }
                        NodeData::Comment { .. } => {}
                        NodeData::Text { contents }
                            if contents
                                .borrow()
                                .chars()
                                .all(|c| matches!(c, '\t' | '\n' | '\x0c' | '\r' | ' ')) => {}
                        _ => return None,
                    }
                }
                element
            });
        *element == Some(Rc::as_ptr(child) as usize)
    }

    fn code_language(&mut self, pre: &Handle, code: &Handle) -> Option<String> {
        fn html_space(c: char) -> bool {
            matches!(c, '\t' | '\n' | '\x0c' | '\r' | ' ')
        }
        fn valid(value: &str) -> bool {
            !value.is_empty()
                && value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_+#./-".contains(&c))
        }
        fn prefixed(class: &str, prefix: &str) -> Option<String> {
            class.split(html_space).find_map(|token| {
                if prefix == "highlight-" && token.starts_with("highlight-source-") {
                    return None;
                }
                token
                    .strip_prefix(prefix)
                    .filter(|value| valid(value))
                    .map(str::to_owned)
            })
        }
        let sources = if Rc::ptr_eq(pre, code) {
            vec![pre]
        } else {
            vec![code, pre]
        };
        for node in sources {
            let class = Self::attr(node, "class").unwrap_or_default();
            if let Some(value) = prefixed(&class, "language-").or_else(|| prefixed(&class, "lang-"))
            {
                return Some(value);
            }
            let data = Self::attr(node, "data-lang").unwrap_or_default();
            let data = data.trim_matches(html_space);
            if valid(data) {
                return Some(data.to_owned());
            }
            if Rc::ptr_eq(node, pre) {
                // ASCII delimiters make every candidate boundary a UTF-8 boundary.
                let bytes = class.as_bytes();
                for start in 0..bytes.len() {
                    if (start == 0 || b"\t\n\x0c\r ;".contains(&bytes[start - 1]))
                        && bytes[start..].starts_with(b"brush:")
                    {
                        let mut at = start + 6;
                        while at < bytes.len() && b"\t\n\x0c\r ".contains(&bytes[at]) {
                            at += 1;
                        }
                        let end = (at..bytes.len())
                            .find(|&i| b"\t\n\x0c\r ;".contains(&bytes[i]))
                            .unwrap_or(bytes.len());
                        let value = &class[at..end];
                        if valid(value) {
                            return Some(value.to_owned());
                        }
                    }
                }
            }
        }
        let parent = parent_handle(pre)?;
        if !self.code_language_wrapper(&parent, pre) {
            return None;
        }
        let class = Self::attr(&parent, "class").unwrap_or_default();
        let highlight = class.split(html_space).any(|token| token == "highlight");
        if highlight {
            if let Some(value) = prefixed(&class, "highlight-source-") {
                return Some(value);
            }
        }
        if class.split(html_space).any(|token| token == "mw-highlight") {
            if let Some(value) = prefixed(&class, "mw-highlight-lang-") {
                return Some(value);
            }
        }
        if highlight {
            let outer = parent_handle(&parent)?;
            if self.code_language_wrapper(&outer, &parent) {
                return prefixed(
                    &Self::attr(&outer, "class").unwrap_or_default(),
                    "highlight-",
                );
            }
        }
        None
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
    fn destination_attr(handle: &Handle, wanted: &str) -> String {
        Self::attr(handle, wanted)
            .unwrap_or_default()
            .trim_matches(|character: char| character.is_ascii_whitespace())
            .to_owned()
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
            cell_wraps: None,
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
            let source = if let NodeData::Element {
                template_contents, ..
            } = &current.data
            {
                template_contents
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| current.clone())
            } else {
                current.clone()
            };
            for child in source.children.borrow().iter() {
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
        let mut seen =
            (held.key_values.len() > 6).then(|| HashSet::with_capacity(held.key_values.len() + 2));
        let mut push = |slot: AttrSlot| {
            let unique = if let Some(seen) = &mut seen {
                seen.insert(slot.clone())
            } else {
                !order.contains(&slot)
            };
            if unique {
                order.push(slot);
            }
        };
        for name in Self::element_attr_names(handle) {
            let slot = match name.as_str() {
                "id" if held.id.is_some() => AttrSlot::Id,
                "class" if !held.classes.is_empty() => AttrSlot::Class,
                other if held.key_values.contains_key(other) => AttrSlot::Key(other.to_string()),
                _ => continue,
            };
            push(slot);
        }
        // A non-empty order is EXHAUSTIVE, so anything the element did not
        // spell under its own name - an attribute renamed or folded on the way
        // in - still has to appear, or the writer drops it silently.
        if held.id.is_some() {
            push(AttrSlot::Id);
        }
        if !held.classes.is_empty() {
            push(AttrSlot::Class);
        }
        for key in held.key_values.keys() {
            let slot = AttrSlot::Key(key.clone());
            push(slot);
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
    /// Safe mode maps horizontal table-cell alignment (carve#2422).
    fn style_slot(&self, tag: &str, property: &str, value: &str) -> Option<StyleSlot> {
        if self.opts.mode == HtmlImportMode::Safe
            && !(is_table_cell(tag) && property == "text-align")
        {
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

    /// Whether `data-delim` IS the list's delimiter: the one PART 10 §12
    /// writes. Any other value is the author's attribute.
    fn reads_delim(ol: &Handle) -> bool {
        Self::attr(ol, "data-delim").as_deref() == Some(")")
    }

    /// Whether an `<ol data-marker>` names the delimiter PART 10 §13a records.
    ///
    /// §13a lists `data-marker` on `<ul>` AND `<ol>`, and an ordered list's
    /// authored marker is its delimiter. carve-php and carve-js both write it
    /// there beside §12's `data-delim`; this engine's writer leaves the
    /// delimiter to §12 alone, so the carrier arrives only in the other two
    /// engines' documents - which is exactly why the READER has to take it.
    fn reads_ordered_marker(ol: &Handle) -> bool {
        Self::carrier(ol, crate::roundtrip::MARKER).as_deref() == Some(")")
    }

    /// Whether `data-task-state` IS the item's state: one PART 10 §11 writes,
    /// an extended state on an EMPTY box or `x` on a CHECKED one
    /// (carve#2887). Anything else is the author's attribute.
    fn reads_task_state(li: &Handle) -> bool {
        let Some(state) = Self::attr(li, "data-task-state") else {
            return false;
        };
        let checked = match state.as_str() {
            "-" | "_" | ">" | "?" => false,
            "x" => true,
            _ => return false,
        };
        li.children.borrow().iter().any(|child| {
            Self::tag(child).as_deref() == Some("input")
                && Self::attr(child, "type").is_some_and(|t| t.eq_ignore_ascii_case("checkbox"))
                && Self::attr(child, "checked").is_some() == checked
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
        mode: HtmlImportMode,
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
                subject: name.to_owned(),
                reason: ": a mapped CSS declaration already sets it",
                severity: HtmlImportSeverity::Info,
                live: false,
            });
        }
        if Self::is_consumed_attribute(handle, tag, name, mode) {
            return None;
        }
        if let Some(canonical) = crate::roundtrip::canonical(name) {
            // A PART 10 §13a CARRIER, which this engine writes under roundtrip
            // export. `data-djot-*` is the deprecated spelling a released
            // carve-php wrote and `canonical` answers for both.
            //
            // RULED on markup-carve/carve#2902 question 5: a reader consumes
            // the inventory and NEVER writes a reserved name back out as an
            // authored attribute, in any mode. Before the ruling carve-php's
            // `safe` mode returned `<hr data-char="_">` as `{data-char=_}` plus
            // `---`, so the carrier reappeared as content. Where the branch
            // building the node consumed it, `is_consumed_attribute` has
            // already answered and this is never reached.
            return Some(Refusal {
                subject: format!("round-trip carrier attribute {canonical}"),
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
                subject: name.to_owned(),
                reason: ": the semantic span's marker owns that key",
                severity: HtmlImportSeverity::Warning,
                live: false,
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
        if value.contains(['\n', '\r']) {
            // A quoted value stops at the line break [CARVE-P4-006], so the
            // written block would not reparse (markup-carve/carve#2385).
            return Some(Refusal {
                subject: name.to_string(),
                reason: ": its value spans a line break, which a Carve attribute value cannot",
                severity: HtmlImportSeverity::Warning,
                live: false,
            });
        }
        None
    }

    /// CONSUMED by the branch that builds this node, and written back from
    /// there. Keeping it as an attribute as well would spell the same string
    /// twice, and diagnosing it would name a loss that does not happen.
    fn is_consumed_attribute(handle: &Handle, tag: &str, name: &str, mode: HtmlImportMode) -> bool {
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
            || (tag == "ol" && name == "data-delim" && Self::reads_delim(handle))
            || Self::reads_carrier(handle, tag, name, mode)
    }

    /// Whether the branch building this node READS the PART 10 §13a carrier
    /// `name`, so the attribute policy neither keeps it nor reports it dropped.
    ///
    /// Raw HTML reconstruction follows the selected trust mode (§13b).
    /// Other recognized carriers retain their grammar checks in every mode.
    fn reads_carrier(handle: &Handle, tag: &str, name: &str, mode: HtmlImportMode) -> bool {
        let Some(canonical) = crate::roundtrip::canonical(name) else {
            return false;
        };
        // §13d: a contradicting pair is consumed whole and restores nothing.
        if Self::contradictory_carriers(handle, tag, mode)
            .is_some_and(|(a, b)| canonical == a || canonical == b)
        {
            return true;
        }
        match canonical {
            crate::roundtrip::MARKER => match tag {
                "ul" => Self::reads_bullet_char(handle).is_some(),
                "ol" => Self::reads_ordered_marker(handle),
                _ => false,
            },
            crate::roundtrip::CHAR => tag == "hr" && Self::reads_break_char(handle).is_some(),
            crate::roundtrip::REF => {
                // An invalid label beside a standing autolink never reaches
                // `reference_label`, so the attribute walk is what reports it.
                matches!(tag, "a" | "img")
                    && Self::attr(handle, name).is_some_and(|v| !v.is_empty())
                    && !(tag == "a" && Self::reads_autolink(handle))
            }
            crate::roundtrip::EXPLICIT_ID => {
                matches!(tag, "section" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
                    && Self::reads_explicit_id(handle)
            }
            crate::roundtrip::AUTOLINK => tag == "a" && Self::reads_autolink(handle),
            crate::roundtrip::XREF => tag == "a" && Self::reads_xref(handle),
            crate::roundtrip::CAPTION_NUMBER => tag == "span" && Self::reads_caption_number(handle),
            crate::roundtrip::RAW => {
                tag == "span" && Self::reads_raw_format(handle, mode).is_some()
            }
            crate::roundtrip::ESCAPED => {
                tag == "span" && Self::reads_escaped(handle, mode).is_some()
            }
            crate::roundtrip::LABEL => tag == "pre" && Self::reads_code_label(handle).is_some(),
            crate::roundtrip::COL_WIDTHS => {
                tag == "table" && Self::reads_separator_widths(handle).is_some()
            }
            crate::roundtrip::GENERATED_ID => {
                matches!(tag, "section" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
                    && Self::reads_generated_section(handle)
            }
            crate::roundtrip::ID_SLOT => {
                tag == "section"
                    && Self::reads_explicit_id(handle)
                    && Self::reads_id_slot(handle).is_some()
            }
            crate::roundtrip::TIGHT => {
                matches!(tag, "ul" | "ol") && Self::reads_tight(handle).is_some()
            }
            crate::roundtrip::LINE_BLOCK => tag == "div",
            crate::roundtrip::DIV => {
                tag == "div" && Self::carrier(handle, crate::roundtrip::LINE_BLOCK).is_none()
            }
            crate::roundtrip::SUBSTITUTION => tag == "span" && Self::reads_substitution(handle),
            _ => false,
        }
    }

    /// The two carriers on this element that contradict each other, where both
    /// values are valid (PART 10 §13d). The footnote pair is resolved where the
    /// endnotes are collected.
    fn contradictory_carriers(
        handle: &Handle,
        tag: &str,
        mode: HtmlImportMode,
    ) -> Option<(&'static str, &'static str)> {
        use crate::roundtrip::{
            AUTOLINK, ESCAPED, EXPLICIT_ID, FOOTNOTE_LABEL, GENERATED_ID, RAW, REF, XREF,
        };
        if tag == "a" && Self::carries_xref(handle) {
            if Self::carries_valid_ref(handle) {
                return Some((XREF, REF));
            }
            if Self::carries_autolink(handle) {
                return Some((XREF, AUTOLINK));
            }
            if Self::carrier(handle, FOOTNOTE_LABEL)
                .is_some_and(|label| valid_footnote_carrier_label(&label))
            {
                return Some((XREF, FOOTNOTE_LABEL));
            }
        }
        match tag {
            "a" if Self::carries_autolink(handle) && Self::carries_valid_ref(handle) => {
                Some((AUTOLINK, REF))
            }
            "section" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                if Self::carries_generated_section(handle) && Self::carries_explicit_id(handle) =>
            {
                Some((GENERATED_ID, EXPLICIT_ID))
            }
            "span"
                if Self::carries_escaped(handle).is_some()
                    && Self::carries_raw_format(handle, mode).is_some() =>
            {
                Some((ESCAPED, RAW))
            }
            _ => None,
        }
    }

    /// One warning for a contradicting pair, which the attribute walk then
    /// consumes without a row of its own.
    fn report_contradictory_carriers(&mut self, handle: &Handle, tag: &str, path: &str) {
        let Some((a, b)) = Self::contradictory_carriers(handle, tag, self.opts.mode) else {
            return;
        };
        self.diag(
            HtmlImportDiagnosticCode::AttributeDropped,
            format!("Dropped contradictory {a} and {b} carriers on <{tag}>"),
            HtmlImportSeverity::Warning,
            path,
            handle,
        );
    }

    /// Presence marks a generated id on a heading or its wrapper.
    fn carries_generated_section(handle: &Handle) -> bool {
        Self::carrier(handle, crate::roundtrip::GENERATED_ID).is_some()
    }

    /// A generated-id carrier applies unless explicit-id contradicts it.
    fn reads_generated_section(section: &Handle) -> bool {
        Self::carries_generated_section(section) && !Self::carries_explicit_id(section)
    }

    /// The authored id slot is decimal and bounded by the heading attributes.
    fn reads_id_slot(section: &Handle) -> Option<usize> {
        if !Self::reads_explicit_id(section) {
            return None;
        }
        let value = Self::carrier(section, crate::roundtrip::ID_SLOT)?;
        let slot = (!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()))
            .then(|| value.parse::<usize>().ok())
            .flatten()?;
        let heading = section
            .children
            .borrow()
            .iter()
            .find(|child| {
                Self::tag(child).is_some_and(|tag| {
                    matches!(tag.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
                })
            })
            .cloned()?;
        let count = Self::element_attr_names(&heading)
            .iter()
            .filter(|name| name.as_str() != "id" && crate::roundtrip::canonical(name).is_none())
            .count();
        (slot <= count).then_some(slot)
    }

    /// Presence takes precedence over paragraph wrappers in the items.
    fn reads_tight(list: &Handle) -> Option<bool> {
        Self::carrier(list, crate::roundtrip::TIGHT).map(|_| true)
    }

    fn reads_code_label(pre: &Handle) -> Option<String> {
        if !pre
            .children
            .borrow()
            .iter()
            .any(|child| Self::tag(child).as_deref() == Some("code"))
        {
            return None;
        }
        Self::carrier(pre, crate::roundtrip::LABEL)
            .filter(|label| !label.contains([']', '\r', '\n']))
    }

    /// The `<table data-carve-col-widths>` separator row, comma-joined.
    ///
    /// PART 10 §13a `CARVE-P10-016`: the WHOLE carrier is refused - no field at
    /// all, so the table comes back in the `|=` form - where any entry is not a
    /// run of decimal digits or exceeds
    /// [`SEPARATOR_WIDTH_LIMIT`](crate::roundtrip::SEPARATOR_WIDTH_LIMIT).
    /// Neither a partial read nor a clamp: both would put a separator row on
    /// the page whose widths nobody wrote.
    fn reads_separator_widths(table: &Handle) -> Option<Vec<u64>> {
        let value = Self::carrier(table, crate::roundtrip::COL_WIDTHS)?;
        if value.trim().is_empty() {
            // An empty value names no width, and a list of none has no
            // separator row to spell.
            return None;
        }
        value
            .split(',')
            .map(|entry| {
                let entry = entry.trim();
                (!entry.is_empty() && entry.bytes().all(|b| b.is_ascii_digit()))
                    .then(|| entry.parse::<u64>().ok())
                    .flatten()
                    .filter(|w| (1..=crate::roundtrip::SEPARATOR_WIDTH_LIMIT).contains(w))
            })
            .collect()
    }

    /// The `<ul data-marker>` bullet, where it names one PART 2 admits and is
    /// not the default. Anything else is the author's attribute.
    fn reads_bullet_char(ul: &Handle) -> Option<char> {
        Self::carrier(ul, crate::roundtrip::MARKER)
            .and_then(|v| single_char(&v))
            .filter(|c| *c == '*')
    }

    /// The `<hr data-char>` break character, where it names one PART 2 admits
    /// and is not the default `-`.
    fn reads_break_char(hr: &Handle) -> Option<char> {
        Self::carrier(hr, crate::roundtrip::CHAR)
            .and_then(|v| single_char(&v))
            .filter(|c| matches!(c, '*' | '_'))
    }

    /// Presence distinguishes an authored ID from a generated heading slug.
    fn reads_explicit_id(handle: &Handle) -> bool {
        Self::carries_explicit_id(handle) && !Self::carries_generated_section(handle)
    }

    fn carries_explicit_id(handle: &Handle) -> bool {
        Self::carrier(handle, crate::roundtrip::EXPLICIT_ID).is_some()
            && Self::attr(handle, "id").is_some_and(|id| !id.is_empty())
    }

    /// Whether the anchor was authored as an autolink. A PRESENCE carrier, so
    /// the value is not compared (§13a), but an anchor with no destination has
    /// no autolink to be.
    fn carries_xref(handle: &Handle) -> bool {
        Self::carrier(handle, crate::roundtrip::XREF).is_some()
            && Self::attr(handle, "href").is_some_and(|href| {
                href.strip_prefix('#')
                    .is_some_and(|id| !id.is_empty() && !id.contains(['>', ' ', '\r', '\n']))
            })
    }

    fn reads_xref(handle: &Handle) -> bool {
        Self::carries_xref(handle)
            && Self::contradictory_carriers(handle, "a", HtmlImportMode::Safe).is_none()
    }

    /// PART 10 §13a `data-carve-line-block`: the `line-block` class is the
    /// node's own output, not an author's. Kept out of the block walk's frame,
    /// which recursion depth multiplies.
    #[inline(never)]
    fn carried_line_block(
        &mut self,
        children: &[Handle],
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let attrs = attrs.and_then(|mut attrs| {
            attrs.classes.retain(|class| class != "line-block");
            if attrs.classes.is_empty() {
                attrs.order.retain(|slot| *slot != AttrSlot::Class);
            }
            (attrs.id.is_some() || !attrs.classes.is_empty() || !attrs.key_values.is_empty())
                .then_some(attrs)
        });
        Ok(vec![BlockNode::LineBlock(LineBlock {
            attrs,
            children: self.blocks(children, path, depth + 1)?,
            lines: None,
            pos: None,
        })])
    }

    /// A bare carrier span holding one bare `<del>` then one bare `<ins>`.
    fn reads_substitution(handle: &Handle) -> bool {
        let bare = |node: &Handle| matches!(&node.data, NodeData::Element { attrs, .. } if attrs.borrow().is_empty());
        let own = matches!(&handle.data, NodeData::Element { attrs, .. } if attrs.borrow().iter().all(|attr| crate::roundtrip::canonical(attr.name.local.as_ref()) == Some(crate::roundtrip::SUBSTITUTION)));
        let children = handle.children.borrow();
        own && Self::carrier(handle, crate::roundtrip::SUBSTITUTION).is_some()
            && children.len() == 2
            && Self::tag(&children[0]).as_deref() == Some("del")
            && Self::tag(&children[1]).as_deref() == Some("ins")
            && children.iter().all(bare)
    }

    fn reads_caption_number(handle: &Handle) -> bool {
        Self::carrier(handle, crate::roundtrip::CAPTION_NUMBER).is_some()
            && matches!(&handle.data, NodeData::Element { attrs, .. } if attrs.borrow().iter().all(|attr| crate::roundtrip::canonical(attr.name.local.as_ref()) == Some(crate::roundtrip::CAPTION_NUMBER)))
            && handle
                .children
                .borrow()
                .iter()
                .all(|child| matches!(child.data, NodeData::Text { .. }))
    }

    fn reads_autolink(a: &Handle) -> bool {
        Self::carries_autolink(a) && !Self::carries_valid_ref(a) && !Self::carries_xref(a)
    }

    fn carries_valid_ref(handle: &Handle) -> bool {
        Self::carrier(handle, crate::roundtrip::REF)
            .is_some_and(|label| crate::parse::reference_label_has_definition_spelling(&label))
    }

    fn carries_autolink(a: &Handle) -> bool {
        Self::carrier(a, crate::roundtrip::AUTOLINK).is_some()
            && Self::attr(a, "href").is_some_and(|href| !href.is_empty())
    }

    /// The `<span data-carve-raw>` format, where the value spells one.
    fn reads_raw_format(span: &Handle, mode: HtmlImportMode) -> Option<String> {
        Self::carries_raw_format(span, mode).filter(|_| Self::carries_escaped(span).is_none())
    }

    fn carries_raw_format(span: &Handle, mode: HtmlImportMode) -> Option<String> {
        Self::carrier(span, crate::roundtrip::RAW)
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            .filter(|v| !v.eq_ignore_ascii_case("html") || mode == HtmlImportMode::Roundtrip)
    }

    /// The one character a `<span data-carve-escaped>` escaped.
    ///
    /// ONE ESCAPE PER ELEMENT (ruling markup-carve/carve#2902 question 4): a
    /// multi-character payload reassembled as `\a\b\c`, which is not valid
    /// Carve, so a span holding a run is not this carrier and keeps its text.
    fn reads_escaped(span: &Handle, mode: HtmlImportMode) -> Option<String> {
        // A raw format this mode declines is no carrier and contradicts nothing.
        Self::carries_escaped(span).filter(|_| Self::carries_raw_format(span, mode).is_none())
    }

    fn carries_escaped(span: &Handle) -> Option<String> {
        Self::carrier(span, crate::roundtrip::ESCAPED)?;
        let text = Self::text(span);
        single_char(&text).map(|c| c.to_string())
    }

    fn reference_label(
        &mut self,
        h: &Handle,
        href: &str,
        title: Option<String>,
        path: &str,
    ) -> Option<String> {
        let label = Self::carrier(h, crate::roundtrip::REF)?;
        if Self::tag(h).as_deref() == Some("a")
            && (Self::carries_autolink(h) || Self::carries_xref(h))
        {
            // §13d: it contradicts the autolink carrier, and `attrs` reported the pair.
            return None;
        }
        let valid = crate::parse::reference_label_has_definition_spelling(&label);
        if !valid {
            self.diag(
                HtmlImportDiagnosticCode::AttributeDropped,
                "Dropped invalid reference carrier".into(),
                HtmlImportSeverity::Warning,
                path,
                h,
            );
            return None;
        }
        if self.writing
            && ((self.cell_depth > 0 && label.contains('|'))
                || !crate::parse::reference_label_has_source_spelling(&label)
                || href.chars().any(char::is_whitespace)
                || title
                    .as_deref()
                    .is_some_and(|value| value.contains(['\r', '\n'])))
        {
            self.diag(
                HtmlImportDiagnosticCode::AttributeDropped,
                "Dropped reference carrier whose fields have no faithful reference source spelling"
                    .into(),
                HtmlImportSeverity::Info,
                path,
                h,
            );
            return None;
        }
        let key = crate::parse::label_key(&label);
        if let Some(index) = self.reference_definition_keys.get(&key) {
            if let BlockNode::LinkReferenceDefinition(def) = &self.carrier_definitions[*index] {
                if def.href != href || def.title != title {
                    self.diag(
                        HtmlImportDiagnosticCode::AttributeDropped,
                        "Dropped reference carrier conflicting with an earlier destination".into(),
                        HtmlImportSeverity::Info,
                        path,
                        h,
                    );
                    return None;
                }
                return Some(def.label.clone());
            }
        }
        if !self.reference_definition_keys.contains_key(&key) {
            self.reference_definition_keys
                .insert(key, self.carrier_definitions.len());
            self.carrier_definitions
                .push(BlockNode::LinkReferenceDefinition(
                    LinkReferenceDefinition {
                        label: label.clone(),
                        href: href.to_owned(),
                        title,
                        attrs: None,
                        pos: None,
                    },
                ));
        }
        Some(label)
    }

    /// A carrier's value under either spelling, canonical first.
    fn carrier(handle: &Handle, canonical: &str) -> Option<String> {
        Self::attr(handle, canonical).or_else(|| {
            let deprecated = canonical.replacen("data-carve-", "data-djot-", 1);
            (deprecated != canonical)
                .then(|| Self::attr(handle, &deprecated))
                .flatten()
        })
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
        // THE PROPERTY NAMES, IN DECLARATION ORDER, not a flag. The shared
        // contract fixtures name the declaration that went nowhere and carry one
        // row per property, so a single generic row cannot say which of two
        // declarations was lost.
        let mut unmapped: Vec<String> = Vec::new();
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
                _ => unmapped.push(property.clone()),
            }
        }
        for property in &unmapped {
            self.diag(
                HtmlImportDiagnosticCode::StyleUnmapped,
                format!("CSS declaration {property} was not mapped"),
                HtmlImportSeverity::Info,
                path,
                handle,
            );
            self.own_last(handle, HtmlImportDiagnosticCode::StyleUnmapped);
        }
    }

    fn attrs(&mut self, handle: &Handle, path: &str) -> Option<Attrs> {
        let tag = Self::tag(handle).unwrap_or_default();
        self.report_contradictory_carriers(handle, &tag, path);
        let mut out = Attrs::default();
        // The keys a mapped declaration fills, so a presentational `align` /
        // `valign` beside one can be refused wherever the source put it.
        let style_filled = self.style_filled_attribute_names(handle, &tag);
        let mut key_order: Vec<String> = Vec::new();
        if let NodeData::Element { attrs, .. } = &handle.data {
            for attr in attrs.borrow().iter() {
                let name = attr.name.local.to_string();
                let value = attr.value.to_string();
                if name != "id" && name != "class" {
                    key_order.push(name.clone());
                }
                if name == "style" {
                    // AHEAD OF THE REFUSAL, deliberately. `refusal` answers for
                    // `style` so the kept-bytes reading goes through the shared
                    // policy, and this element is one the import rewrites - the
                    // CSS mapping is its reading, not a drop.
                    self.map_style(handle, path, &tag, &value, &mut out);
                } else if let Some(refusal) =
                    Self::refusal(handle, &tag, &name, &value, &style_filled, self.opts.mode)
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
                } else if Self::is_consumed_attribute(handle, &tag, &name, self.opts.mode) {
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
        // Keys in the order the element spells them, as `fmt` keeps them
        // (carve-rs#2012). Every name is listed, so a key written back later
        // (a folded `title`) lands where the element had it; a name with no
        // value is skipped by the writer. Writing exit only (carve#1647).
        if self.writing && !out.key_values.is_empty() {
            out.order = [AttrSlot::Id, AttrSlot::Class]
                .into_iter()
                .chain(key_order.into_iter().map(AttrSlot::Key))
                .collect();
        }
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
            if let Some(name) = self.preceding_label_text(handle, label_class) {
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
            let mut ids = self.admonition_title_ids.borrow_mut();
            let ids = ids.get_or_insert_with(|| {
                AdmonitionTitleIndex::new(handle, &self.detached_footnote_blocks)
            });
            return ids.title_id(handle).map(|id| vec![("id", vec![id])]);
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
    fn preceding_label_text(&self, handle: &Handle, label_class: &str) -> Option<String> {
        let parent = parent_handle(handle)?;
        let siblings = parent.children.borrow();
        let at = sibling_position(&self.sibling_index, &parent, &siblings, handle)?;
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
        let mut entries = self.index_backref_entries.borrow_mut();
        let (term, ordinals) = entries.entry(node_key(&parent)).or_insert_with(|| {
            let mut term = String::new();
            let mut ordinals = HashMap::new();
            for node in parent.children.borrow().iter() {
                if Self::tag(node).as_deref() == Some("a") && has_class(node, "index-backref") {
                    ordinals.insert(node_key(node), ordinals.len() + 1);
                } else {
                    term.push_str(&Self::flat_text(node));
                }
            }
            (term.trim().to_string(), ordinals)
        });
        let ordinal = ordinals.get(&node_key(handle))?;
        if term.is_empty() {
            return None;
        }
        let label = self.label(LABEL_INDEX_BACKREF);
        let held_name = Self::attr(handle, "aria-label")?;
        if held_name.len() < label.len() + term.len() + 1 {
            return None;
        }
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
                || (tag == "template"
                    && Self::carrier(handle, crate::roundtrip::ABBREVIATIONS).is_some())
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
                let produced = self.block(handle, &path, depth + 1)?;
                // Carve source has no boundary between two definition lists, so
                // an attribute-less one following another joins it (carve#2369).
                // `produced` can hold two itself: a leading `<dd>`'s list lands
                // ahead of the list its `<dl>` builds.
                for block in produced {
                    match (out.last_mut(), block) {
                        (
                            Some(BlockNode::DefinitionList(previous)),
                            BlockNode::DefinitionList(next),
                        ) if next.attrs.is_none() => {
                            previous.items.extend(next.items);
                            previous.loose |= next.loose;
                            self.diag(
                                HtmlImportDiagnosticCode::ElementUnwrapped,
                                "Merged <dl> into the definition list before it: Carve source has no boundary between two adjacent definition lists".into(),
                                HtmlImportSeverity::Info,
                                &path,
                                handle,
                            );
                        }
                        (_, block) => out.push(block),
                    }
                }
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
            && inline.iter().all(|h| {
                matches!(&h.data, NodeData::Comment { .. })
                    || dom_text_is_layout_only(h)
                    || Self::is_active_tag(h)
            });
        if comments_only {
            // An active element imports to nothing, so it cannot make the
            // comment beside it inline (markup-carve/carve-rs#2029).
            for (handle, path) in inline.iter().zip(inline_paths.iter()) {
                // Charged as the inline walk would have charged it.
                self.enter(depth + 1)?;
                if Self::is_active_tag(handle) {
                    let tag = Self::tag(handle).unwrap_or_default();
                    self.diag(
                        HtmlImportDiagnosticCode::ElementDropped,
                        format!("Dropped active <{tag}> element"),
                        HtmlImportSeverity::Warning,
                        path,
                        handle,
                    );
                } else if let NodeData::Comment { contents } = &handle.data {
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
    /// Where the walk's reportable state stands, for a measurement that rewinds.
    fn mark(&self) -> ImporterMark {
        ImporterMark {
            carrier_definitions: self.carrier_definitions.len(),
            nodes: self.nodes,
            diagnostics: self.diagnostics.len(),
            turned_away: self.turned_away,
            unspellable: self.unspellable.len(),
            displaced_figure_attrs: self.displaced_figure_attrs.len(),
            lone_image_paragraphs: self.lone_image_paragraphs.len(),
            empty_code_spans: self.empty_code_spans.len(),
            braced_kind_spans: self.braced_kind_spans.len(),
        }
    }

    /// Undo everything a marked measurement recorded. See [`Importer::mark`].
    fn restore(&mut self, mark: ImporterMark) {
        self.carrier_definitions.truncate(mark.carrier_definitions);
        self.reference_definition_keys
            .retain(|_, index| *index < mark.carrier_definitions);
        self.nodes = mark.nodes;
        self.diagnostics.truncate(mark.diagnostics);
        self.turned_away = mark.turned_away;
        self.unspellable.truncate(mark.unspellable);
        self.displaced_figure_attrs
            .truncate(mark.displaced_figure_attrs);
        self.lone_image_paragraphs
            .truncate(mark.lone_image_paragraphs);
        self.empty_code_spans.truncate(mark.empty_code_spans);
        self.braced_kind_spans.truncate(mark.braced_kind_spans);
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

/// An image's alternative text as the content it unwraps to.
fn image_alt_content(h: &Handle) -> Vec<InlineNode> {
    let alt = Importer::attr(h, "alt").unwrap_or_default();
    if alt.is_empty() {
        Vec::new()
    } else {
        vec![InlineNode::text(alt)]
    }
}

/// The single character `value` spells, or `None` for an empty or longer one.
///
/// A PART 10 §13a carrier whose row names one character holds exactly one, so
/// a longer value is a MALFORMED carrier and not this engine's to reassemble.
fn single_char(value: &str) -> Option<char> {
    let mut chars = value.chars();
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

fn names_no_destination(value: Option<&str>) -> bool {
    value.is_none()
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
    let collapsible = |ch| is_layout_space(ch) && ch != '\t';
    let value = s
        .split(collapsible)
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
        if s.starts_with(collapsible) { " " } else { "" },
        value,
        if s.ends_with(collapsible) { " " } else { "" }
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
fn trim_edge_whitespace(nodes: Vec<InlineNode>) -> Vec<InlineNode> {
    trim_formatting_edges(nodes, true, true)
}

fn trim_formatting_edges(
    mut nodes: Vec<InlineNode>,
    leading: bool,
    trailing: bool,
) -> Vec<InlineNode> {
    if leading {
        while let Some(InlineNode::Text(first)) = nodes.first_mut() {
            first.value = first.value.trim_start_matches(is_layout_space).to_string();
            if first.value.is_empty() {
                nodes.remove(0);
            } else {
                break;
            }
        }
    }
    if trailing {
        while let Some(InlineNode::Text(last)) = nodes.last_mut() {
            last.value = last.value.trim_end_matches(is_layout_space).to_string();
            if last.value.is_empty() {
                nodes.pop();
            } else {
                break;
            }
        }
    }
    fn ends_with_space(node: &InlineNode) -> bool {
        let children = match node {
            InlineNode::Text(text) => return text.value.ends_with([' ', '\t']),
            InlineNode::Emphasis(node) => &node.children,
            InlineNode::CriticInsert(node) => &node.children,
            InlineNode::CriticDelete(node) => &node.children,
            InlineNode::Link(node) => &node.children,
            InlineNode::Span(node) => &node.children,
            _ => return false,
        };
        children.last().is_some_and(ends_with_space)
    }
    for index in 0..nodes.len() {
        let before = if index == 0 {
            leading
        } else {
            matches!(nodes[index - 1], InlineNode::HardBreak(_))
                || ends_with_space(&nodes[index - 1])
        };
        let after = match nodes.get(index + 1) {
            None => trailing,
            Some(InlineNode::HardBreak(_)) => true,
            Some(InlineNode::Text(text)) => text.value.starts_with([' ', '\t']),
            _ => false,
        };
        let children = match &mut nodes[index] {
            InlineNode::Emphasis(node) => &mut node.children,
            InlineNode::CriticInsert(node) => &mut node.children,
            InlineNode::CriticDelete(node) => &mut node.children,
            _ => continue,
        };
        if children.iter().all(|child| matches!(child, InlineNode::Text(text) if text.value.chars().all(is_layout_space))) {
            continue;
        }
        *children = trim_formatting_edges(std::mem::take(children), before, after);
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
fn drop_space_after_hard_break(nodes: Vec<InlineNode>) -> Vec<InlineNode> {
    let mut kept = Vec::with_capacity(nodes.len());
    for mut node in nodes {
        if matches!(kept.last(), Some(InlineNode::HardBreak(_))) {
            if let InlineNode::Text(text) = &mut node {
                text.value = text.value.trim_start_matches(is_layout_space).to_string();
                if text.value.is_empty() {
                    continue;
                }
            }
        }
        kept.push(node);
    }
    kept
}

/// A link's or span's edge whitespace stands outside it, as one space that
/// merges with whitespace already there (markup-carve/carve#2361).
/// Whitespace-only content stays, and so does U+00A0, which is content.
fn hoist_edge_space(nodes: Vec<InlineNode>) -> Vec<InlineNode> {
    hoist_nested_edge_space(nodes, false)
}

fn hoist_nested_edge_space(nodes: Vec<InlineNode>, within_link_or_span: bool) -> Vec<InlineNode> {
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
            InlineNode::Link(link) if !within_link_or_span => {
                link.children = coalesce(hoist_nested_edge_space(
                    std::mem::take(&mut link.children),
                    true,
                ));
                hoist(&mut link.children)
            }
            InlineNode::Span(span) if !within_link_or_span => {
                span.children = coalesce(hoist_nested_edge_space(
                    std::mem::take(&mut span.children),
                    true,
                ));
                hoist(&mut span.children)
            }
            InlineNode::Emphasis(emphasis) if within_link_or_span => {
                emphasis.children = coalesce(hoist_nested_edge_space(
                    std::mem::take(&mut emphasis.children),
                    true,
                ));
                hoist(&mut emphasis.children)
            }
            InlineNode::CriticInsert(change) if within_link_or_span => {
                change.children = coalesce(hoist_nested_edge_space(
                    std::mem::take(&mut change.children),
                    true,
                ));
                hoist(&mut change.children)
            }
            InlineNode::CriticDelete(change) if within_link_or_span => {
                change.children = coalesce(hoist_nested_edge_space(
                    std::mem::take(&mut change.children),
                    true,
                ));
                hoist(&mut change.children)
            }
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
    fn read(nodes: &[Handle], text: &mut String, spaced: &mut bool) -> bool {
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
                    if !read(std::slice::from_ref(first), text, spaced) {
                        return false;
                    }
                }
                Some("mrow" | "mstyle" | "mpadded") => {
                    if !read(&node.children.borrow(), text, spaced) {
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
                    let token = collapse(&token);
                    let token = token.trim_matches(' ');
                    // A space keeps two words or numbers apart, so 1, space, 2 is not 12.
                    if *spaced
                        && text.chars().last().is_some_and(char::is_alphanumeric)
                        && token.chars().next().is_some_and(char::is_alphanumeric)
                    {
                        text.push(' ');
                    }
                    text.push_str(token);
                    *spaced = false;
                }
                Some("mspace") => *spaced = true,
                _ => return false,
            }
        }
        true
    }
    let mut text = String::new();
    let mut spaced = false;
    (read(&math.children.borrow(), &mut text, &mut spaced) && !text.is_empty()).then_some(text)
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

/// Only an image has an attribute slot of its own under a caption line; any
/// other target shares the figure's line, merged the way the parser merges two
/// stacked lines (markup-carve/carve#2370).
fn share_figure_attrs(figure: &mut Figure) {
    let slot = match &mut *figure.target {
        FigureTarget::Image(_) => return,
        FigureTarget::BlockQuote(quote) => &mut quote.attrs,
        FigureTarget::Table(table) => &mut table.attrs,
        FigureTarget::CodeBlock(code) => &mut code.attrs,
        FigureTarget::Paragraph(paragraph) => &mut paragraph.attrs,
    };
    let Some(mut merged) = slot.take() else {
        return;
    };
    if let Some(own) = figure.attrs.take() {
        let mut shared = Some(merged);
        crate::parse::merge_leading_attrs(&mut shared, own);
        merged = shared.expect("a merge keeps the attributes it was given");
    }
    figure.attrs = Some(merged);
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
    if !nodes.iter_mut().any(|node| {
        matches!(node, InlineNode::Code(code) if code.value.is_empty())
            || crate::render_carve::empty_code_run_children_mut(node).is_some()
    }) {
        return;
    }
    let original = std::mem::take(nodes);
    let mut reads_on = vec![false; original.len() + 1];
    let mut blank_rest = vec![true; original.len() + 1];
    for i in (0..original.len()).rev() {
        reads_on[i] = reads_on[i + 1]
            || !matches!(&original[i], InlineNode::Text(text) if text.value.is_empty());
        blank_rest[i] = blank_rest[i + 1]
            && matches!(&original[i], InlineNode::Text(text) if text.value.trim().is_empty());
    }
    nodes.reserve(original.len());
    let mut join = false;
    for (index, mut node) in original.into_iter().enumerate() {
        let after = followed || reads_on[index + 1];
        if let InlineNode::Code(code) = &mut node {
            if code.value.is_empty() {
                let ends = crate::render_carve::empty_code_position_ends_its_run(
                    followed || (after && !blank_rest[index + 1]),
                    labelled,
                    cell_not_last,
                );
                if !keep(code, ends) {
                    join = true;
                    continue;
                }
                if ends && writing {
                    append_inline_boundary(nodes, node, join);
                    break;
                }
            }
        } else if let Some((braced, children)) =
            crate::render_carve::empty_code_run_children_mut(&mut node)
        {
            let held = !children.is_empty();
            settle_empty_code_spans(
                children,
                if braced { false } else { after },
                if braced { labelled } else { true },
                cell_not_last,
                writing,
                keep,
                emptied,
            );
            if braced && held && children.is_empty() {
                emptied();
                join = true;
                continue;
            }
        }
        append_inline_boundary(nodes, node, join);
        join = false;
    }
}

pub(crate) const NESTED_SAME_KIND_UNWRAPPED: &str = "Unwrapped an inline element nested in one of its own kind: a span inside a span of the same kind has no Carve spelling, so its content is written in the outer one";

/// Unwrap each inner span of a same-kind nesting the writer refuses (ruling
/// markup-carve/carve#2066), naming the candidate marks it took.
///
/// THE CONDITION IS THE WRITER'S, asked by rendering rather than guessed from
/// the shape: `note_braced_span` reports the spans it could not spell, and only
/// those come off. A structural test would also have to model which delimiters
/// a kind shares (`BoldItalic` carries two) and that the braced form shares the
/// stack, and a second reading of that rule is the thing worth not writing.
///
/// Every span must carry a candidate mark, which is what makes it addressable
/// here: the HTML importer stamps one as it builds, and
/// [`unwrap_same_kind_spans_for_writing`] stamps a document that has none.
fn unwrap_refused_same_kind_spans(document: &mut Document) -> Vec<usize> {
    let mut nested = false;
    let mut probe = |nodes: &mut Vec<InlineNode>, _: bool| {
        nested |= holds_same_kind_nesting(nodes, &mut Vec::new());
    };
    for blocks in document.footnote_defs.values_mut() {
        for_each_inline_run(blocks, &mut probe);
    }
    for_each_inline_run(&mut document.children, &mut probe);
    if !nested || render_carve(document).is_ok() {
        return Vec::new();
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
    unwrapped
}

fn unwrap_nested_same_kind_spans(document: &mut Document, importer: &mut Importer) {
    for index in unwrap_refused_same_kind_spans(document) {
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

/// The same unwrapping for a document built without candidate marks, as one
/// loss message per span taken.
///
/// An importer that hands the writer's refusal to the caller converts NOTHING:
/// `carve migrate --from markdown` exited 2 and wrote no file for a document
/// whose only unspellable construct was one `*(*foo*)*` (carve-rs#2098). The
/// ruling the HTML importer already follows (markup-carve/carve#2066) answers
/// it, so the two importers answer it the same way and with the same code.
pub(crate) fn unwrap_same_kind_spans_for_writing(document: &mut Document) -> Vec<String> {
    let mut next = 0usize;
    let mut stamp = |nodes: &mut Vec<InlineNode>, _: bool| stamp_span_marks(nodes, &mut next);
    for blocks in document.footnote_defs.values_mut() {
        for_each_inline_run(blocks, &mut stamp);
    }
    for_each_inline_run(&mut document.children, &mut stamp);
    let unwrapped = unwrap_refused_same_kind_spans(document);
    for blocks in document.footnote_defs.values_mut() {
        for_each_inline_run(blocks, &mut |nodes, _| strip_span_marks(nodes));
    }
    for_each_inline_run(&mut document.children, &mut |nodes, _| {
        strip_span_marks(nodes)
    });
    vec![NESTED_SAME_KIND_UNWRAPPED.to_owned(); unwrapped.len()]
}

/// Give every span under `nodes` a candidate mark, numbering from `next`.
fn stamp_span_marks(nodes: &mut [InlineNode], next: &mut usize) {
    for node in nodes {
        if let InlineNode::Emphasis(Emphasis { pos, .. })
        | InlineNode::CriticInsert(CriticInsert { pos, .. })
        | InlineNode::CriticDelete(CriticDelete { pos, .. }) = node
        {
            *pos = Some(candidate_mark(*next));
            *next += 1;
        }
        if let Some((_, children)) = crate::render_carve::empty_code_run_children_mut(node) {
            stamp_span_marks(children, next);
        }
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
    let original = std::mem::take(nodes);
    nodes.reserve(original.len());
    let mut join = false;
    for mut node in original {
        if let Some((_, children)) = crate::render_carve::empty_code_run_children_mut(&mut node) {
            take_span_marks(children, unwrap);
        }
        let mark = match &node {
            InlineNode::Emphasis(Emphasis { pos, .. })
            | InlineNode::CriticInsert(CriticInsert { pos, .. })
            | InlineNode::CriticDelete(CriticDelete { pos, .. }) => {
                pos.as_ref().map(|pos| pos.start_offset)
            }
            _ => None,
        };
        if mark.map(&mut *unwrap).unwrap_or(false) {
            let children = match node {
                InlineNode::Emphasis(e) => e.children,
                InlineNode::CriticInsert(i) => i.children,
                InlineNode::CriticDelete(d) => d.children,
                _ => unreachable!(),
            };
            let mut children = children.into_iter();
            if let Some(first) = children.next() {
                append_inline_boundary(nodes, first, true);
                nodes.extend(children);
            }
            join = true;
        } else {
            append_inline_boundary(nodes, node, join);
            join = false;
        }
    }
}

/// Join text only across a boundary exposed by a removal or unwrapping.
fn append_inline_boundary(nodes: &mut Vec<InlineNode>, node: InlineNode, join: bool) {
    if join {
        if let (Some(InlineNode::Text(previous)), InlineNode::Text(next)) =
            (nodes.last_mut(), &node)
        {
            previous.value.push_str(&next.value);
            return;
        }
    }
    nodes.push(node);
}

/// Every inline sequence under `blocks`, with whether it is a table cell that
/// is not the last in its row. EVERY VARIANT, WITH NO WILDCARD ARM, for the
/// reason `take_candidate_marks` gives.
pub(crate) fn for_each_inline_run(
    blocks: &mut [BlockNode],
    f: &mut impl FnMut(&mut Vec<InlineNode>, bool),
) {
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
        carrier_definitions: Vec::new(),
        reference_definition_keys: HashMap::new(),
        opts: options,
        writing,
        diagnostics: Vec::new(),
        turned_away: None,
        document_order: HashMap::new(),
        code_language_wrappers: HashMap::new(),
        nodes: 0,
        quote_depth: 0,
        cell_depth: 0,
        heading_depth: 0,
        unspellable: Vec::new(),
        displaced_figure_attrs: Vec::new(),
        lone_image_paragraphs: Vec::new(),
        empty_code_spans: Vec::new(),
        braced_kind_spans: Vec::new(),
        footnote_refs: HashMap::new(),
        detached_footnote_blocks: HashSet::new(),
        admonition_title_ids: RefCell::new(None),
        formula_images: HashSet::new(),
        sibling_index: RefCell::new(HashMap::new()),
        index_backref_entries: RefCell::new(HashMap::new()),
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
    importer.sibling_index.borrow_mut().clear();
    importer.index_backref_entries.borrow_mut().clear();
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
    if !empty_code_spans.is_empty() {
        for blocks in footnote_defs.values_mut() {
            for_each_inline_run(blocks, &mut settle);
        }
        for_each_inline_run(&mut children, &mut settle);
    }
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
    children.append(&mut importer.carrier_definitions);
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
    crate::ast_json::hydrate_resolved_empty_references(&mut document);
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

/// The pipe-table grid, written as the `::: list-table` it is equivalent to
/// (docs/html-import-contract.md, "A table whose cells hold blocks can be
/// written as a list table").
fn list_table_of(
    rows: Vec<TableRow>,
    own_alignment: &[Vec<(Option<TableAlign>, Option<TableVerticalAlign>)>],
    caption: Option<Vec<InlineNode>>,
    attrs: Option<Attrs>,
    writing: bool,
) -> Admonition {
    // A placeholder is a header cell when the cell it continues is one.
    let mut header: Vec<Vec<bool>> = Vec::with_capacity(rows.len());
    for (r, row) in rows.iter().enumerate() {
        let mut own: Vec<bool> = Vec::with_capacity(row.cells.len());
        for (c, cell) in row.cells.iter().enumerate() {
            let value = match cell.span {
                Some(TableCellSpan::Rowspan) => {
                    r > 0 && header[r - 1].get(c).copied().unwrap_or(false)
                }
                Some(TableCellSpan::Colspan) => c > 0 && own[c - 1],
                None => cell.header,
            };
            own.push(value);
        }
        header.push(own);
    }
    let leading = |r: usize| header[r].iter().take_while(|&&h| h).count();
    let mut header_rows = 0;
    while header_rows < rows.len() && leading(header_rows) == rows[header_rows].cells.len() {
        header_rows += 1;
    }
    let header_cols = (header_rows..rows.len()).map(leading).min().unwrap_or(0);

    // The alignment each source cell was imported with, in grid order.
    let mut own = own_alignment.iter();
    let mut items = Vec::with_capacity(rows.len());
    for (r, row) in rows.into_iter().enumerate() {
        let mut own_row = own.next().map(|v| v.iter()).into_iter().flatten();
        let mut cells = Vec::with_capacity(row.cells.len());
        for (c, cell) in row.cells.into_iter().enumerate() {
            if let Some(span) = cell.span {
                let marker = if span == TableCellSpan::Rowspan {
                    "^"
                } else {
                    "<"
                };
                cells.push(list_item(
                    vec![paragraph(vec![InlineNode::Text(Text {
                        value: marker.into(),
                        pos: None,
                    })])],
                    None,
                ));
                continue;
            }
            let (align, valign) = own_row.next().copied().unwrap_or((None, None));
            let mut children = cell.blocks.unwrap_or_default();
            // The escape is the writer's: a bare `^` or `<` item is a span
            // marker. The published tree holds the text, as it does for a
            // pipe-table cell.
            if let (true, [BlockNode::Paragraph(p)]) = (writing, children.as_mut_slice()) {
                if p.attrs.is_none() {
                    if let [InlineNode::Text(t)] = p.children.as_slice() {
                        if t.value == "^" || t.value == "<" {
                            p.children = vec![InlineNode::EscapedText(EscapedText {
                                value: t.value.clone(),
                                pos: None,
                            })];
                        }
                    }
                }
            }
            let mut item_attrs = cell.attrs.unwrap_or_default();
            let mut put = |key: &str, value: Option<String>| {
                let Some(value) = value else { return };
                if item_attrs.key_values.contains_key(key) {
                    return;
                }
                item_attrs.key_values.insert(key.to_owned(), value);
                if writing {
                    push_order(&mut item_attrs, key);
                }
            };
            if cell.header && r >= header_rows && c >= header_cols {
                put("header", Some(String::new()));
            }
            put(
                "align",
                align.map(|a| {
                    match a {
                        TableAlign::Left => "left",
                        TableAlign::Right => "right",
                        TableAlign::Center => "center",
                    }
                    .to_owned()
                }),
            );
            put(
                "valign",
                valign.map(|a| {
                    match a {
                        TableVerticalAlign::Top => "top",
                        TableVerticalAlign::Middle => "middle",
                        TableVerticalAlign::Bottom => "bottom",
                    }
                    .to_owned()
                }),
            );
            let attrs = (item_attrs != Attrs::default()).then_some(item_attrs);
            cells.push(list_item(children, attrs));
        }
        let tight = !cells.iter().any(|item| item.children.len() > 1);
        items.push(list_item(vec![bullet_list(cells, tight)], None));
    }
    let mut table_attrs = attrs.unwrap_or_default();
    for (key, n) in [("header-rows", header_rows), ("header-cols", header_cols)] {
        if n > 0 {
            table_attrs.key_values.insert(key.to_owned(), n.to_string());
            if writing {
                push_order(&mut table_attrs, key);
            }
        }
    }
    Admonition {
        attrs: (table_attrs != Attrs::default()).then_some(table_attrs),
        kind: "list-table".into(),
        title: caption.filter(|c| !c.is_empty()),
        label: None,
        children: vec![bullet_list(items, true)],
        pos: None,
    }
}

/// Record `key` as the last slot, on the writing exit only: `order` is a
/// source-layout field. An empty order means "id, classes, keys in sorted
/// order", so that reading is spelled out before the key is appended.
fn push_order(attrs: &mut Attrs, key: &str) {
    if attrs.order.is_empty() {
        if attrs.id.is_some() {
            attrs.order.push(AttrSlot::Id);
        }
        if !attrs.classes.is_empty() {
            attrs.order.push(AttrSlot::Class);
        }
        for existing in attrs.key_values.keys() {
            if existing != key {
                attrs.order.push(AttrSlot::Key(existing.clone()));
            }
        }
    }
    attrs.order.push(AttrSlot::Key(key.to_owned()));
}

fn paragraph(children: Vec<InlineNode>) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        attrs: None,
        children,
        at_content_column: true,
        block_image: false,
        pos: None,
    })
}

fn list_item(children: Vec<BlockNode>, attrs: Option<Attrs>) -> ListItem {
    ListItem {
        attrs,
        checked: None,
        task_state: None,
        children,
        pos: None,
    }
}

fn bullet_list(items: Vec<ListItem>, tight: bool) -> BlockNode {
    BlockNode::List(List {
        attrs: None,
        ordered: false,
        start: None,
        ol_type: None,
        bare_marker: false,
        delim: None,
        bullet_char: None,
        tight,
        items,
        pos: None,
    })
}

/// The block-level elements whose boundary an inline flatten keeps as a
/// separator, including the ones never top level (carve-js `isFlattenedBlock`).
fn is_flattened_block(h: &Handle) -> bool {
    Importer::tag(h).is_some_and(|tag| {
        Importer::is_block_tag(&tag)
            || matches!(
                tag.as_str(),
                "li" | "dt" | "dd" | "td" | "th" | "tr" | "caption" | "figcaption"
            )
    })
}

/// Whether a flattened block boundary needs a separator: not where either side
/// is missing, is a hard break, or already has layout at the join. A no-break
/// space is content (PART 11 §7), so it does not separate.
fn needs_separator(before: &[InlineNode], after: &[InlineNode]) -> bool {
    let (Some(last), Some(first)) = (before.last(), after.first()) else {
        return false;
    };
    if matches!(last, InlineNode::HardBreak(_)) || matches!(first, InlineNode::HardBreak(_)) {
        return false;
    }
    if matches!(last, InlineNode::Text(t) if t.value.ends_with([' ', '\t'])) {
        return false;
    }
    !matches!(first, InlineNode::Text(t) if t.value.starts_with([' ', '\t']))
}

fn text_ends_with_layout(node: Option<&InlineNode>) -> bool {
    matches!(node, Some(InlineNode::Text(t)) if t.value.ends_with([' ', '\t']))
}
