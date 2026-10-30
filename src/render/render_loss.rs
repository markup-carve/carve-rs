//! Loss reports for output formats that cannot preserve every AST detail.

use crate::ast::{BlockNode, Document, InlineNode, OrderedListType, Pos, Ruby};
use std::cell::RefCell;
use std::collections::BTreeMap;

/// Default maximum number of retained render losses.
pub const DEFAULT_MAX_RENDER_LOSSES: usize = 100;

/// Output format checked for render losses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderTarget {
    /// HTML output.
    Html,
    /// Markdown output.
    Markdown,
    /// Plain-text output.
    Plain,
    /// ANSI terminal output.
    Ansi,
    /// Canonical Carve source output.
    Carve,
}

impl RenderTarget {
    /// Return the lowercase target name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Markdown => "markdown",
            Self::Plain => "plain",
            Self::Ansi => "ansi",
            Self::Carve => "carve",
        }
    }
}

/// Whether a raw source node is inline or block content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawNodeType {
    /// Raw inline content.
    Inline,
    /// Raw block content.
    Block,
}

impl RawNodeType {
    /// Return the lowercase node-type name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Block => "block",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum MarkdownLoss {
    Container,
    Caption,
    Attributes,
    FenceTitle,
    RawHtml,
}

impl MarkdownLoss {
    fn details(self) -> (&'static str, &'static str) {
        match self {
            Self::Container => (
                "markdown-container-flattened",
                "Flattened a container into Markdown blocks",
            ),
            Self::Caption => (
                "markdown-caption-flattened",
                "Wrote a caption as ordinary Markdown text",
            ),
            Self::Attributes => (
                "markdown-attributes-dropped",
                "Dropped block attributes while rendering Markdown",
            ),
            Self::FenceTitle => (
                "fence-title-dropped",
                "Dropped a code fence title without a language in Markdown",
            ),
            Self::RawHtml => (
                "raw-html-escaped",
                "Escaped raw HTML while rendering Markdown",
            ),
        }
    }
}

/// One detail that an output target would discard or change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderLoss {
    /// Stable loss code.
    pub code: &'static str,
    /// Format of a raw node, when relevant.
    pub format: Option<String>,
    /// Attribute affected by the loss, when relevant.
    pub attribute: Option<String>,
    /// Style affected by the loss, when relevant.
    pub style: Option<&'static str>,
    /// The authored ordered-list delimiter, for `list-delimiter-changed`.
    pub delim: Option<char>,
    /// The delimiter written instead, for `list-delimiter-changed`.
    pub emitted_delim: Option<char>,
    /// Column span affected by the loss, when relevant.
    pub colspan: Option<usize>,
    /// Row span affected by the loss, when relevant.
    pub rowspan: Option<usize>,
    /// Output target that incurred the loss.
    pub target: RenderTarget,
    /// Raw node class, when the loss concerns raw content.
    pub node_type: RawNodeType,
    /// Source span, when available.
    pub pos: Option<Pos>,
    /// Included file identity, when the loss came from an include.
    pub file: Option<String>,
    /// Directives leading to the included content.
    pub included_by: Option<Vec<crate::includes::IncludeSite>>,
    /// Human-readable explanation.
    pub message: String,
}

/// Rendered value and a bounded report of losses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderResult<T> {
    /// Rendered output.
    pub value: T,
    /// Retained losses in encounter order.
    pub losses: Vec<RenderLoss>,
    /// Number of losses before truncation.
    pub total_losses: usize,
    /// Loss counts by stable code.
    pub totals_by_code: BTreeMap<&'static str, usize>,
    /// Whether the retention limit hid further losses.
    pub truncated: bool,
}

/// Strictness and retention limit for checked rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckedRenderOptions {
    /// Return an error when the target would lose content.
    pub strict: bool,
    /// Maximum number of loss records to retain.
    pub max_losses: usize,
}

impl Default for CheckedRenderOptions {
    fn default() -> Self {
        Self {
            strict: false,
            max_losses: DEFAULT_MAX_RENDER_LOSSES,
        }
    }
}

/// Error returned when strict rendering would lose content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderLossError {
    /// Retained losses in encounter order.
    pub losses: Vec<RenderLoss>,
    /// Number of losses before truncation.
    pub total_losses: usize,
    /// Loss counts by stable code.
    pub totals_by_code: BTreeMap<&'static str, usize>,
    /// Whether the retention limit hid further losses.
    pub truncated: bool,
}

impl std::fmt::Display for RenderLossError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "render would lose {} node{}",
            self.total_losses,
            if self.total_losses == 1 { "" } else { "s" }
        )
    }
}
impl std::error::Error for RenderLossError {}

struct Collector {
    target: RenderTarget,
    max: usize,
    total: usize,
    losses: Vec<RenderLoss>,
    totals_by_code: BTreeMap<&'static str, usize>,
}
thread_local! { static COLLECTOR: RefCell<Option<Collector>> = const { RefCell::new(None) }; }

pub(crate) struct LossCheckpoint(Option<(usize, usize, BTreeMap<&'static str, usize>)>);

pub(crate) fn checkpoint() -> LossCheckpoint {
    LossCheckpoint(COLLECTOR.with(|slot| {
        slot.borrow().as_ref().map(|collector| {
            (
                collector.total,
                collector.losses.len(),
                collector.totals_by_code.clone(),
            )
        })
    }))
}

impl LossCheckpoint {
    pub(crate) fn restore(self) {
        COLLECTOR.with(|slot| {
            let mut slot = slot.borrow_mut();
            match self.0 {
                None => *slot = None,
                Some((total, retained, totals_by_code)) => {
                    let collector = slot
                        .as_mut()
                        .expect("render retry keeps its collector scope");
                    collector.total = total;
                    collector.losses.truncate(retained);
                    collector.totals_by_code = totals_by_code;
                }
            }
        });
    }
}

pub(crate) fn record_markdown_loss(kind: MarkdownLoss, node_type: RawNodeType, pos: Option<&Pos>) {
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut().filter(|c| c.target == RenderTarget::Markdown) else {
            return;
        };
        let (code, message) = kind.details();
        c.total += 1;
        *c.totals_by_code.entry(code).or_default() += 1;
        if c.losses.len() < c.max {
            c.losses.push(RenderLoss {
                code,
                format: None,
                attribute: None,
                style: None,
                delim: None,
                emitted_delim: None,
                colspan: None,
                rowspan: None,
                target: RenderTarget::Markdown,
                node_type,
                pos: pos.cloned(),
                file: None,
                included_by: None,
                message: message.to_string(),
            });
        }
    });
}

pub(crate) fn collect_markdown_unattached(spans: Vec<Pos>) {
    collect_markdown_block_spans(
        spans,
        "Dropped an unattached block attribute while rendering Markdown",
    );
}

pub(crate) fn collect_markdown_promoted_image_runs(spans: Vec<Pos>) {
    let (_, message) = MarkdownLoss::Attributes.details();
    collect_markdown_block_spans(spans, message);
}

fn collect_markdown_block_spans(spans: Vec<Pos>, message: &str) {
    if spans.is_empty() {
        return;
    }
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut().filter(|c| c.target == RenderTarget::Markdown) else {
            return;
        };
        let (code, _) = MarkdownLoss::Attributes.details();
        c.total += spans.len();
        *c.totals_by_code.entry(code).or_default() += spans.len();
        let mut dangling = spans;
        dangling.sort_by_key(|pos| pos.start_offset);
        let mut rendered = std::mem::take(&mut c.losses).into_iter().peekable();
        let mut dangling = dangling.into_iter().peekable();
        let mut merged = Vec::with_capacity(c.max.min(c.total));
        let mut rendered_offset = 0;
        while merged.len() < c.max && (rendered.peek().is_some() || dangling.peek().is_some()) {
            if let Some(loss) = rendered.peek() {
                rendered_offset = rendered_offset.max(
                    loss.pos
                        .as_ref()
                        .map_or(rendered_offset, |p| p.start_offset),
                );
            }
            if dangling
                .peek()
                .is_some_and(|pos| pos.start_offset < rendered_offset)
                || rendered.peek().is_none()
            {
                let pos = dangling.next().expect("dangling loss available");
                merged.push(RenderLoss {
                    code,
                    format: None,
                    attribute: None,
                    style: None,
                    delim: None,
                    emitted_delim: None,
                    colspan: None,
                    rowspan: None,
                    target: RenderTarget::Markdown,
                    node_type: RawNodeType::Block,
                    pos: Some(pos),
                    file: None,
                    included_by: None,
                    message: message.to_string(),
                });
            } else if let Some(loss) = rendered.next() {
                merged.push(loss);
            }
        }
        c.losses = merged;
    });
}

pub(crate) fn record_table_span(cell: &crate::ast::TableCell) {
    let colspan = cell.colspan.filter(|count| *count > 1);
    let rowspan = cell.rowspan.filter(|count| *count > 1);
    if colspan.is_none() && rowspan.is_none() {
        return;
    }
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        c.total += 1;
        *c.totals_by_code.entry("table-span-flattened").or_default() += 1;
        if c.losses.len() < c.max {
            c.losses.push(RenderLoss {
                code: "table-span-flattened",
                format: None,
                attribute: None,
                style: None,
                delim: None,
                emitted_delim: None,
                colspan,
                rowspan,
                target: c.target,
                node_type: RawNodeType::Block,
                pos: cell.pos.clone(),
                file: None,
                included_by: None,
                message: "Flattened a spanning table cell into separate cells".to_string(),
            });
        }
    });
}

pub(crate) fn record_table_header_row(pos: Option<&Pos>) {
    record_table_header_loss(
        "table-header-row-demoted",
        "Wrote a later header row as a body row",
        pos,
    );
}

pub(crate) fn record_table_header_row_hoisted(pos: Option<&Pos>) {
    record_table_header_loss(
        "table-header-row-hoisted",
        "Moved a header row above earlier body rows",
        pos,
    );
}

pub(crate) fn record_table_header_cell(pos: Option<&Pos>) {
    record_table_header_loss(
        "table-header-cell-demoted",
        "Wrote a header cell as a body cell",
        pos,
    );
}

fn record_table_header_loss(code: &'static str, message: &str, pos: Option<&Pos>) {
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        c.total += 1;
        *c.totals_by_code.entry(code).or_default() += 1;
        if c.losses.len() < c.max {
            c.losses.push(RenderLoss {
                code,
                format: None,
                attribute: None,
                style: None,
                delim: None,
                emitted_delim: None,
                colspan: None,
                rowspan: None,
                target: c.target,
                node_type: RawNodeType::Block,
                pos: pos.cloned(),
                file: None,
                included_by: None,
                message: message.to_string(),
            });
        }
    });
}

pub(crate) fn record_list_tightness_lost(pos: Option<&Pos>) {
    record_table_header_loss(
        "list-tightness-lost",
        "Wrote a nested list item loose to preserve its blocks",
        pos,
    );
}

pub(crate) fn record_raw_drop(format: &str, node_type: RawNodeType, pos: Option<Pos>) {
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        c.total += 1;
        *c.totals_by_code.entry("raw-format-dropped").or_default() += 1;
        if c.losses.len() < c.max {
            c.losses.push(RenderLoss {
                code: "raw-format-dropped",
                attribute: None,
                style: None,
                delim: None,
                emitted_delim: None,
                colspan: None,
                rowspan: None,
                format: Some(format.to_string()),
                target: c.target,
                node_type,
                pos,
                file: None,
                included_by: None,
                message: format!(
                    "Dropped {} raw format {:?} while rendering {}",
                    node_type.as_str(),
                    format,
                    c.target.as_str()
                ),
            });
        }
    });
}

/// Which sink a blanked destination came from.
///
/// PART 9 §25 makes the row's `message` NORMATIVE and spells one string per
/// sink. `target` already carries the render target and `nodeType` is `inline`
/// for both sinks, so the message is the only field left that says whether a
/// clickable destination or an image source was defused
/// (markup-carve/carve#2686).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeniedSink {
    /// A link or autolink destination.
    Destination,
    /// An image source.
    ImageSource,
}

impl DeniedSink {
    /// The verbatim message the spec names. Nothing is appended.
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Destination => "Blanked a denied destination scheme",
            Self::ImageSource => "Blanked a denied image source",
        }
    }
}

/// One row per destination the PART 9 §25 sink denylist blanked, in the order
/// the renderer emitted them. Every clickable sink is an inline node, and the
/// emitted value does not change: this is what a checked render says about the
/// link it defused (markup-carve/carve#2681).
pub(crate) fn record_destination_denied(pos: Option<&Pos>, sink: DeniedSink) {
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        c.total += 1;
        *c.totals_by_code.entry("destination-denied").or_default() += 1;
        if c.losses.len() < c.max {
            c.losses.push(RenderLoss {
                code: "destination-denied",
                attribute: None,
                style: None,
                delim: None,
                emitted_delim: None,
                colspan: None,
                rowspan: None,
                format: None,
                target: c.target,
                node_type: RawNodeType::Inline,
                pos: pos.cloned(),
                file: None,
                included_by: None,
                message: sink.message().to_string(),
            });
        }
    });
}

/// One row per authored attribute a PART 10 §13a carrier displaced.
///
/// RULED on markup-carve/carve#2902 question 1: the carrier wins, the authored
/// attribute is dropped, and the drop is REPORTED naming the attribute. Before
/// the ruling carve-php emitted both, which is invalid HTML, with the
/// precedence differing per carrier.
pub(crate) fn record_carrier_collision(
    carrier: &str,
    authored: &str,
    pos: Option<&Pos>,
    node_type: RawNodeType,
) {
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        c.total += 1;
        *c.totals_by_code
            .entry("carrier-attribute-dropped")
            .or_default() += 1;
        if c.losses.len() < c.max {
            c.losses.push(RenderLoss {
                code: "carrier-attribute-dropped",
                attribute: Some(authored.to_owned()),
                style: None,
                delim: None,
                emitted_delim: None,
                colspan: None,
                rowspan: None,
                format: None,
                target: c.target,
                node_type,
                pos: pos.cloned(),
                file: None,
                included_by: None,
                message: format!(
                    "Dropped authored attribute {authored:?}: the roundtrip carrier {carrier:?} owns that name"
                ),
            });
        }
    });
}

pub(crate) fn record_ruby_flattened(ruby: &Ruby) {
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        c.total += 1;
        *c.totals_by_code.entry("ruby-flattened").or_default() += 1;
        if c.losses.len() < c.max {
            c.losses.push(RenderLoss {
                code: "ruby-flattened",
                attribute: None,
                style: None,
                delim: None,
                emitted_delim: None,
                colspan: None,
                rowspan: None,
                format: None,
                target: c.target,
                node_type: RawNodeType::Inline,
                pos: ruby.pos.clone(),
                file: None,
                included_by: None,
                message: "Flattened ruby annotation".to_string(),
            });
        }
    });
}

/// One row per editorial comment a text target writes as bare text, in
/// document order (markup-carve/carve#2791).
pub(crate) fn record_editorial_comment_flattened(pos: Option<&Pos>) {
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        c.total += 1;
        *c.totals_by_code
            .entry("editorial-comment-flattened")
            .or_default() += 1;
        if c.losses.len() < c.max {
            c.losses.push(RenderLoss {
                code: "editorial-comment-flattened",
                attribute: None,
                style: None,
                delim: None,
                emitted_delim: None,
                colspan: None,
                rowspan: None,
                format: None,
                target: c.target,
                node_type: RawNodeType::Inline,
                pos: pos.cloned(),
                file: None,
                included_by: None,
                message: "Flattened an editorial comment into the surrounding text".to_string(),
            });
        }
    });
}

pub(crate) fn record_definition_list_flattened(pos: Option<&Pos>) {
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        c.total += 1;
        *c.totals_by_code
            .entry("definition-list-flattened")
            .or_default() += 1;
        if c.losses.len() < c.max {
            c.losses.push(RenderLoss {
                code: "definition-list-flattened",
                attribute: None,
                style: None,
                delim: None,
                emitted_delim: None,
                colspan: None,
                rowspan: None,
                format: None,
                target: c.target,
                node_type: RawNodeType::Block,
                pos: pos.cloned(),
                file: None,
                included_by: None,
                message: "Flattened a nested definition list without carrier markers".to_string(),
            });
        }
    });
}

pub(crate) fn record_list_style_dropped(style: OrderedListType, pos: Option<&Pos>) {
    let style = match style {
        OrderedListType::LowerAlpha => "lower-alpha",
        OrderedListType::UpperAlpha => "upper-alpha",
        OrderedListType::LowerRoman => "lower-roman",
        OrderedListType::UpperRoman => "upper-roman",
    };
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        c.total += 1;
        *c.totals_by_code.entry("list-style-dropped").or_default() += 1;
        if c.losses.len() < c.max {
            c.losses.push(RenderLoss {
                code: "list-style-dropped",
                attribute: None,
                style: Some(style),
                delim: None,
                emitted_delim: None,
                colspan: None,
                rowspan: None,
                format: None,
                target: c.target,
                node_type: RawNodeType::Block,
                pos: pos.cloned(),
                file: None,
                included_by: None,
                message: format!(
                    "Wrote a {style} ordered list as decimal while rendering markdown"
                ),
            });
        }
    });
}

pub(crate) fn record_ruby_in_document(doc: &Document) {
    if !COLLECTOR.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|c| c.target == RenderTarget::Carve)
    }) {
        return;
    }
    struct RubyVisitor;
    impl crate::include_walk::SubtreeVisitor for RubyVisitor {
        fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
            for block in blocks {
                crate::include_walk::visit_block_children(block, self);
            }
        }
        fn inlines(&mut self, inlines: &mut Vec<InlineNode>) {
            for inline in inlines {
                if matches!(inline, InlineNode::CitationGroup(_)) {
                    continue; // The Carve writer emits group.raw, not its item fields.
                }
                if let InlineNode::Ruby(ruby) = inline {
                    record_ruby_flattened(ruby);
                }
                crate::include_walk::visit_inline_children(inline, self);
            }
        }
    }
    use crate::include_walk::SubtreeVisitor as _;
    let mut copy = doc.clone();
    let mut visitor = RubyVisitor;
    visitor.blocks(&mut copy.children);
    for blocks in copy.footnote_defs.values_mut() {
        visitor.blocks(blocks);
    }
}

/// Collect actual losses produced while `render` runs. This is the checked
/// entry point for custom renderers and options-taking render pipelines.
pub fn with_render_loss_report<T>(
    target: RenderTarget,
    options: CheckedRenderOptions,
    render: impl FnOnce() -> T,
) -> Result<RenderResult<T>, RenderLossError> {
    COLLECTOR.with(|slot| {
        assert!(
            slot.borrow().is_none(),
            "checked renders cannot be nested on one thread"
        );
        *slot.borrow_mut() = Some(Collector {
            target,
            max: options.max_losses,
            total: 0,
            losses: Vec::new(),
            totals_by_code: BTreeMap::new(),
        });
    });
    let value = render();
    let mut collector = COLLECTOR.with(|slot| slot.borrow_mut().take().unwrap());
    for loss in &mut collector.losses {
        if let Some(file) = loss.pos.as_ref().and_then(|pos| pos.file.as_ref()) {
            if let Some(sites) = file.include_sites().filter(|sites| !sites.is_empty()) {
                loss.file = Some(file.as_str().to_string());
                loss.included_by = Some(sites.to_vec());
            }
        }
    }
    let truncated = collector.total > collector.losses.len();
    if options.strict && collector.total > 0 {
        Err(RenderLossError {
            losses: collector.losses,
            total_losses: collector.total,
            totals_by_code: collector.totals_by_code,
            truncated,
        })
    } else {
        Ok(RenderResult {
            value,
            losses: collector.losses,
            total_losses: collector.total,
            totals_by_code: collector.totals_by_code,
            truncated,
        })
    }
}

pub(crate) use with_render_loss_report as checked;

pub(crate) fn record_list_delimiter_changed(delim: char, emitted: char, pos: Option<&Pos>) {
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        c.total += 1;
        *c.totals_by_code.entry("list-delimiter-changed").or_default() += 1;
        if c.losses.len() < c.max {
            c.losses.push(RenderLoss {
                code: "list-delimiter-changed",
                attribute: None,
                style: None,
                delim: Some(delim),
                emitted_delim: Some(emitted),
                colspan: None,
                rowspan: None,
                format: None,
                target: c.target,
                node_type: RawNodeType::Block,
                pos: pos.cloned(),
                file: None,
                included_by: None,
                message: format!(
                    "Changed an ordered list delimiter from {delim} to {emitted} while rendering markdown"
                ),
            });
        }
    });
}
