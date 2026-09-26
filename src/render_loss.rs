use crate::ast::{BlockNode, Document, InlineNode, Pos, Ruby};
use std::cell::RefCell;
use std::collections::BTreeMap;

pub const DEFAULT_MAX_RENDER_LOSSES: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderTarget {
    Html,
    Markdown,
    Plain,
    Ansi,
    Carve,
}

impl RenderTarget {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawNodeType {
    Inline,
    Block,
}

impl RawNodeType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Block => "block",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderLoss {
    pub code: &'static str,
    pub format: Option<String>,
    pub target: RenderTarget,
    pub node_type: RawNodeType,
    pub pos: Option<Pos>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderResult<T> {
    pub value: T,
    pub losses: Vec<RenderLoss>,
    pub total_losses: usize,
    pub totals_by_code: BTreeMap<&'static str, usize>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckedRenderOptions {
    pub strict: bool,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderLossError {
    pub losses: Vec<RenderLoss>,
    pub total_losses: usize,
    pub totals_by_code: BTreeMap<&'static str, usize>,
    pub truncated: bool,
}

impl std::fmt::Display for RenderLossError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "render would drop {} raw node{}",
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

pub(crate) fn record_raw_drop(format: &str, node_type: RawNodeType, pos: Option<Pos>) {
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        c.total += 1;
        *c.totals_by_code.entry("raw-format-dropped").or_default() += 1;
        if c.losses.len() < c.max {
            c.losses.push(RenderLoss {
                code: "raw-format-dropped",
                format: Some(format.to_string()),
                target: c.target,
                node_type,
                pos,
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

pub(crate) fn record_ruby_flattened(ruby: &Ruby) {
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        c.total += 1;
        *c.totals_by_code.entry("ruby-flattened").or_default() += 1;
        if c.losses.len() < c.max {
            c.losses.push(RenderLoss {
                code: "ruby-flattened",
                format: None,
                target: c.target,
                node_type: RawNodeType::Inline,
                pos: ruby.pos.clone(),
                message: "Flattened ruby annotation".to_string(),
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
    let collector = COLLECTOR.with(|slot| slot.borrow_mut().take().unwrap());
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

pub(crate) fn record_table_section_attributes(node: &crate::ast::Table) {
    let Some(groups) = &node.row_groups else {
        return;
    };
    let mut fields = vec![
        ("rowGroups.headAttrs".to_owned(), &groups.head_attrs),
        ("rowGroups.footAttrs".to_owned(), &groups.foot_attrs),
    ];
    fields.extend(
        groups
            .bodies
            .iter()
            .enumerate()
            .map(|(i, b)| (format!("rowGroups.bodies[{i}].attrs"), &b.attrs)),
    );
    for (field, attrs) in fields {
        if !attrs
            .as_ref()
            .is_some_and(|a| *a != crate::ast::Attrs::default())
        {
            continue;
        }
        COLLECTOR.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(c) = slot.as_mut() else {
                return;
            };
            c.total += 1;
            *c.totals_by_code
                .entry("table-section-attributes-dropped")
                .or_default() += 1;
            if c.losses.len() < c.max {
                c.losses.push(RenderLoss {
                    code: "table-section-attributes-dropped",
                    format: None,
                    target: c.target,
                    node_type: RawNodeType::Block,
                    pos: node.pos.clone(),
                    message: format!("Dropped {field} while rendering {}", c.target.as_str()),
                });
            }
        });
    }
}
