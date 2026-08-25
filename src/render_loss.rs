use crate::ast::Pos;
use std::cell::RefCell;

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
    pub format: String,
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
}
thread_local! { static COLLECTOR: RefCell<Option<Collector>> = const { RefCell::new(None) }; }

pub(crate) fn record_raw_drop(format: &str, node_type: RawNodeType, pos: Option<Pos>) {
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut() else { return };
        c.total += 1;
        if c.losses.len() < c.max {
            c.losses.push(RenderLoss {
                code: "raw-format-dropped",
                format: format.to_string(),
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
        });
    });
    let value = render();
    let collector = COLLECTOR.with(|slot| slot.borrow_mut().take().unwrap());
    let truncated = collector.total > collector.losses.len();
    if options.strict && collector.total > 0 {
        Err(RenderLossError {
            losses: collector.losses,
            total_losses: collector.total,
            truncated,
        })
    } else {
        Ok(RenderResult {
            value,
            losses: collector.losses,
            total_losses: collector.total,
            truncated,
        })
    }
}

pub(crate) use with_render_loss_report as checked;
