//! The typed refusal a renderer owes its caller at the depth ceiling (PART 9
//! §25).
//!
//! §25 gives every renderer a bound above the parser's and says what happens AT
//! it: the render MUST fail with a typed, documented error naming the bound -
//! the same rule PART 12 §9(b) already applies to ingest, at the other end of
//! the same pipe. Returning empty output instead is the failure the clause was
//! written against: the caller gets a string that looks complete and has had its
//! body deleted (carve-rs#511 item 5). carve-js raises `RenderDepthError` and
//! carve-php `RenderDepthExceededException`; a `Result` is the same statement in
//! this language.
//!
//! It costs nothing on any path a document travels. The ceiling exceeds
//! `parse::MAX_NESTING_DEPTH` by construction, so a tree that came from the
//! parser cannot reach it - which is why the source-level `to_*` entry points
//! keep returning `String`. What is left is a tree built through the API or read
//! by `from_json`, where the caller is the one who can act on the error.
//!
//! The bound is recorded through a thread-local rather than by threading a
//! `Result` through every recursive renderer function, following
//! [`crate::abbr_budget`]: the guard is installed for one render and unwinds on
//! drop, so a nested render (a block extension rendering sub-blocks) stacks
//! correctly instead of reporting its parent's state.

use std::cell::Cell;
use std::fmt;

/// A render refused because the tree is deeper than the renderer's ceiling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderDepthError {
    renderer: &'static str,
    limit: usize,
}

impl RenderDepthError {
    pub(crate) fn new(renderer: &'static str, limit: usize) -> Self {
        Self { renderer, limit }
    }

    /// Which target refused: `"html"`, `"markdown"`, `"plain"`, `"ansi"` or
    /// `"carve"`.
    pub fn renderer(&self) -> &'static str {
        self.renderer
    }

    /// The bound that was reached, in AST levels.
    pub fn limit(&self) -> usize {
        self.limit
    }
}

impl fmt::Display for RenderDepthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the {} renderer refused: the tree is deeper than its ceiling of {} levels",
            self.renderer, self.limit
        )
    }
}

impl std::error::Error for RenderDepthError {}

thread_local! {
    /// The renderer that reached the ceiling during the render currently running
    /// on this thread, if any. `None` outside a render, and while one is running
    /// until a guard site records.
    static REACHED: Cell<Option<&'static str>> = const { Cell::new(None) };
}

/// Record that this renderer reached the ceiling.
///
/// Called at each depth guard, beside the early return that keeps the recursion
/// bounded. The guard still returns - the bound is what stops the stack from
/// growing - and the top-level entry point turns the record into the error.
pub(crate) fn record(renderer: &'static str) {
    REACHED.with(|cell| {
        if cell.get().is_none() {
            cell.set(Some(renderer));
        }
    });
}

/// RAII watch installed for one render, restoring the previous value on drop so
/// nested renders stack and unwind (the reason spelled out in the module note).
pub(crate) struct RenderDepthWatch {
    previous: Option<&'static str>,
}

impl RenderDepthWatch {
    pub(crate) fn new() -> Self {
        let previous = REACHED.with(|cell| cell.replace(None));
        RenderDepthWatch { previous }
    }

    /// The render's output, or the refusal if any guard recorded one.
    pub(crate) fn into_result(self, output: String) -> Result<String, RenderDepthError> {
        match REACHED.with(|cell| cell.get()) {
            Some(renderer) => Err(RenderDepthError::new(
                renderer,
                crate::render::MAX_RENDER_DEPTH,
            )),
            None => Ok(output),
        }
    }
}

impl Drop for RenderDepthWatch {
    fn drop(&mut self) {
        REACHED.with(|cell| cell.set(self.previous));
    }
}
