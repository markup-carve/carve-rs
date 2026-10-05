//! Typed refusals from the canonical Carve writer.

use std::cell::{Cell, RefCell};
use std::fmt;

/// The canonical writer cannot spell an AST node without changing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUnspellable {
    node_type: &'static str,
    reason: &'static str,
}

impl SourceUnspellable {
    pub(crate) fn new(node_type: &'static str, reason: &'static str) -> Self {
        Self { node_type, reason }
    }

    pub fn node_type(&self) -> &'static str {
        self.node_type
    }
    pub fn reason(&self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for SourceUnspellable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the Carve renderer cannot spell {}: {}",
            self.node_type, self.reason
        )
    }
}

impl std::error::Error for SourceUnspellable {}

/// A typed reason the canonical Carve writer refused a tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderCarveError {
    Depth(crate::RenderDepthError),
    SourceUnspellable(SourceUnspellable),
}

impl fmt::Display for RenderCarveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Depth(error) => error.fmt(f),
            Self::SourceUnspellable(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for RenderCarveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Depth(error) => Some(error),
            Self::SourceUnspellable(error) => Some(error),
        }
    }
}

impl From<crate::RenderDepthError> for RenderCarveError {
    fn from(error: crate::RenderDepthError) -> Self {
        Self::Depth(error)
    }
}

/// Why the canonical Carve writer produced no source.
///
/// `--carve` and `carve fmt` parse and re-serialize a whole document, so they
/// can be stopped by the profile the options carry OR by the writer itself. The
/// writer's refusal used to be unwrapped with an `expect` claiming a parsed tree
/// never reaches it. That claim holds for the DEPTH ceiling, which sits above
/// the parse cap, and not for a round-trip refusal: one level past the cap the
/// over-cap flattening hands the writer a code node whose value carries the
/// ladder's indentation, which the block layer strips on the way back out, and
/// the CLI aborted with exit 101 on valid input (carve-rs#2326).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarveWriteError {
    /// The profile refused a node, or the source exceeded its `max_length`.
    Profile(crate::ProfileViolationError),
    /// The writer cannot spell the tree the parser built.
    Render(RenderCarveError),
}

impl fmt::Display for CarveWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Profile(error) => error.fmt(f),
            Self::Render(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for CarveWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Profile(error) => Some(error),
            Self::Render(error) => Some(error),
        }
    }
}

impl From<crate::ProfileViolationError> for CarveWriteError {
    fn from(error: crate::ProfileViolationError) -> Self {
        Self::Profile(error)
    }
}

impl From<RenderCarveError> for CarveWriteError {
    fn from(error: RenderCarveError) -> Self {
        Self::Render(error)
    }
}

thread_local! {
    static UNSPELLABLE: Cell<Option<(&'static str, &'static str)>> = const { Cell::new(None) };
    static NESTED_SAME_KIND: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// The inner spans of an unspellable same-kind nesting, for the HTML importer.
pub(crate) fn record_nested_same_kind(spans: Vec<usize>) {
    NESTED_SAME_KIND.with(|cell| cell.borrow_mut().extend(spans));
}

pub(crate) fn take_nested_same_kind() -> Vec<usize> {
    NESTED_SAME_KIND.with(|cell| std::mem::take(&mut *cell.borrow_mut()))
}

pub(crate) fn record_unspellable(node_type: &'static str, reason: &'static str) {
    UNSPELLABLE.with(|cell| {
        if cell.get().is_none() {
            cell.set(Some((node_type, reason)));
        }
    });
}

pub(crate) struct SourceSpellWatch {
    previous: Option<(&'static str, &'static str)>,
}

impl SourceSpellWatch {
    pub(crate) fn new() -> Self {
        NESTED_SAME_KIND.with(|cell| cell.borrow_mut().clear());
        Self {
            previous: UNSPELLABLE.with(|cell| cell.replace(None)),
        }
    }

    pub(crate) fn error(&self) -> Option<RenderCarveError> {
        UNSPELLABLE.with(|cell| {
            cell.get().map(|(node_type, reason)| {
                RenderCarveError::SourceUnspellable(SourceUnspellable::new(node_type, reason))
            })
        })
    }
}

impl Drop for SourceSpellWatch {
    fn drop(&mut self) {
        UNSPELLABLE.with(|cell| cell.set(self.previous));
    }
}
