//! Typed refusals from the canonical Carve writer.

use std::cell::Cell;
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

thread_local! {
    static UNSPELLABLE: Cell<Option<(&'static str, &'static str)>> = const { Cell::new(None) };
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
