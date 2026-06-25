//! Bounds the cumulative bytes that `::: index` block rendering may contribute
//! to a single render, defending against a memory-amplification DoS.
//!
//! The index extension re-emits the COMPLETE sorted backlink list in every
//! `::: index` block. With `K` index blocks and `N` total `:index[term]`
//! markers in a document, the HTML output is `K * N * ~52` bytes - both `K` and
//! `N` are attacker-controlled content, so a small input can amplify into a huge
//! output (e.g. a 57KB input expanding to ~130MB, a ~2255x blowup).
//!
//! Policy (mirrors `abbr_budget`): the cumulative bytes emitted by `::: index`
//! rendering are capped at
//! `max(INDEX_EXPANSION_BUDGET_BASE, INDEX_EXPANSION_BUDGET_FACTOR * input_len)`.
//! Once emitting the next index entry or backlink would exceed the budget, that
//! entry (and every later one) is dropped - no huge string is ever allocated.
//! The budget sits far above any legitimate document (and above every corpus
//! fixture, so the corpus is unaffected).
//!
//! The remaining budget lives in a thread-local installed for the duration of a
//! single render by [`IndexBudgetGuard`] (RAII). This keeps the bound cumulative
//! across every `::: index` block of the render without threading a counter
//! through the renderer, and avoids leaking state between successive renders on
//! the same thread.

use std::cell::Cell;

/// Budget floor: index rendering may always contribute at least this many bytes,
/// regardless of how small the input was.
pub(crate) const INDEX_EXPANSION_BUDGET_BASE: usize = 1_000_000;

/// Budget scales with input size at this factor, so a genuinely large document
/// with a large legitimate index is not clipped.
pub(crate) const INDEX_EXPANSION_BUDGET_FACTOR: usize = 8;

thread_local! {
    /// Remaining index-expansion bytes for the render currently running on this
    /// thread. `None` means no render is active (calls to `try_spend` then
    /// conservatively use the floor budget).
    static REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Compute the index expansion budget for an input of `input_len` bytes.
fn budget_for(input_len: usize) -> usize {
    INDEX_EXPANSION_BUDGET_BASE.max(INDEX_EXPANSION_BUDGET_FACTOR.saturating_mul(input_len))
}

/// RAII guard that installs the index-expansion budget for one render and
/// restores the previous value on drop (so nested renders correctly stack and
/// unwind).
pub(crate) struct IndexBudgetGuard {
    previous: Option<usize>,
}

impl IndexBudgetGuard {
    /// Install a budget sized for an input of `input_len` bytes.
    pub(crate) fn new(input_len: usize) -> Self {
        let previous = REMAINING.with(|cell| cell.replace(Some(budget_for(input_len))));
        IndexBudgetGuard { previous }
    }
}

impl Drop for IndexBudgetGuard {
    fn drop(&mut self) {
        REMAINING.with(|cell| cell.set(self.previous));
    }
}

/// Try to spend `cost` index-expansion bytes against the active render budget.
///
/// Returns `true` (and deducts) when the emission fits; returns `false`
/// (exhausting the budget) once it would overflow, signalling the caller to drop
/// this and all subsequent index content. When no guard is active (a renderer
/// invoked without one), the floor budget is used so the bound still applies.
pub(crate) fn try_spend(cost: usize) -> bool {
    REMAINING.with(|cell| {
        let remaining = cell.get().unwrap_or(INDEX_EXPANSION_BUDGET_BASE);
        if cost > remaining {
            // Exhaust the budget so every later entry/backlink also drops.
            cell.set(Some(0));
            return false;
        }
        cell.set(Some(remaining - cost));
        true
    })
}

/// Whether the active render budget is already fully spent.
///
/// A cheap peek (no allocation, no deduction) so the caller can bail out before
/// doing expensive work - e.g. escaping a large index term - that `try_spend`
/// would only then reject. Once exhausted the budget never refills within a
/// render, so a later `::: index` block need not even format its first entry.
pub(crate) fn is_exhausted() -> bool {
    REMAINING.with(|cell| cell.get().unwrap_or(INDEX_EXPANSION_BUDGET_BASE) == 0)
}
