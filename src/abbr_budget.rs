//! Bounds the cumulative bytes that abbreviation expansion may contribute to a
//! single render, defending against a memory-amplification DoS.
//!
//! Each occurrence of an abbreviation re-emits its full expansion (the `title`
//! attribute in HTML/Markdown, `(EXPANSION)` in ANSI). A document with a large
//! definition (`*[HT]: <huge>`) and many `HT` occurrences would otherwise emit
//! `expansion_len * occurrence_count` bytes, far larger than the input.
//!
//! Policy (shared across the HTML, Markdown, and ANSI renderers for
//! cross-engine consistency): the cumulative expansion bytes are capped at
//! `max(ABBR_EXPANSION_BUDGET_BASE, ABBR_EXPANSION_BUDGET_FACTOR * input_len)`.
//! Once emitting the next occurrence's expansion would exceed the budget, that
//! occurrence (and every later one) degrades to its plain key text only - no
//! `<abbr>` wrapper, no title - so no huge string is ever allocated. The budget
//! sits far above any legitimate document (and above every corpus fixture, so
//! the corpus is unaffected).
//!
//! The remaining budget lives in a thread-local installed for the duration of a
//! single render by [`AbbrBudgetGuard`] (RAII). This keeps the bound cumulative
//! across every block of the render without threading a counter through the many
//! renderer functions, and avoids leaking state between successive renders on
//! the same thread.

use std::cell::Cell;

/// Budget floor: abbreviation expansion may always contribute at least this
/// many bytes, regardless of how small the input was.
pub(crate) const ABBR_EXPANSION_BUDGET_BASE: usize = 1_000_000;

/// Budget scales with input size at this factor, so a genuinely large document
/// with many legitimate abbreviations is not clipped.
pub(crate) const ABBR_EXPANSION_BUDGET_FACTOR: usize = 8;

thread_local! {
    /// Remaining abbreviation-expansion bytes for the render currently running
    /// on this thread. `None` means no render is active (calls to `try_spend`
    /// then conservatively use the floor budget).
    static REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Compute the expansion budget for an input of `input_len` bytes.
fn budget_for(input_len: usize) -> usize {
    ABBR_EXPANSION_BUDGET_BASE.max(ABBR_EXPANSION_BUDGET_FACTOR.saturating_mul(input_len))
}

/// RAII guard that installs the abbreviation-expansion budget for one render and
/// restores the previous value on drop (so nested renders - e.g. a block
/// extension that renders sub-blocks - correctly stack and unwind).
pub(crate) struct AbbrBudgetGuard {
    previous: Option<usize>,
}

impl AbbrBudgetGuard {
    /// Install a budget sized for an input of `input_len` bytes.
    pub(crate) fn new(input_len: usize) -> Self {
        let previous = REMAINING.with(|cell| cell.replace(Some(budget_for(input_len))));
        AbbrBudgetGuard { previous }
    }
}

impl Drop for AbbrBudgetGuard {
    fn drop(&mut self) {
        REMAINING.with(|cell| cell.set(self.previous));
    }
}

/// Try to spend `cost` expansion bytes against the active render budget.
///
/// Returns `true` (and deducts) when the expansion fits; returns `false`
/// (exhausting the budget) once it would overflow, signalling the caller to
/// degrade this and all subsequent occurrences to plain key text. When no guard
/// is active (a renderer invoked without one), the floor budget is used so the
/// bound still applies.
pub(crate) fn try_spend(cost: usize) -> bool {
    REMAINING.with(|cell| {
        let remaining = cell.get().unwrap_or(ABBR_EXPANSION_BUDGET_BASE);
        if cost > remaining {
            // Exhaust the budget so every later occurrence also degrades.
            cell.set(Some(0));
            return false;
        }
        cell.set(Some(remaining - cost));
        true
    })
}
