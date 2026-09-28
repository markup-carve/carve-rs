//! Bounds the cumulative bytes that DERIVED-TEXT EXPANSION may contribute to a
//! single render, defending against a memory-amplification DoS.
//!
//! Two constructs republish text they did not pay for, and both charge here:
//!
//! - An abbreviation occurrence re-emits its full expansion (the `title`
//!   attribute in HTML/Markdown, `(EXPANSION)` in ANSI). A document with a large
//!   definition (`*[HT]: <huge>`) and many `HT` occurrences would otherwise emit
//!   `expansion_len * occurrence_count` bytes, far larger than the input.
//! - A cross-reference republishes its target heading's whole display text while
//!   the reference itself costs only the slug, so K references to one long
//!   heading emit `K * heading_len` bytes (`markup-carve/carve-rs#805`).
//!
//! They share one budget because they amplify the same output through the same
//! renderers; a second mechanism would be a second thing to get wrong.
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

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

/// Budget floor: abbreviation expansion may always contribute at least this
/// many bytes, regardless of how small the input was.
pub(crate) const ABBR_EXPANSION_BUDGET_BASE: usize = 1_000_000;

/// Budget scales with input size at this factor, so a genuinely large document
/// with many legitimate abbreviations is not clipped.
pub(crate) const ABBR_EXPANSION_BUDGET_FACTOR: usize = 8;

thread_local! {
    #[cfg(test)]
    static MEASURED_LABEL_BYTES: Cell<usize> = const { Cell::new(0) };
    /// Remaining abbreviation-expansion bytes for the render currently running
    /// on this thread. `None` means no render is active (calls to `try_spend`
    /// then conservatively use the floor budget).
    static REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
    /// What each cross-reference target's label cost the FIRST time it was
    /// rendered. See [`label_rejected`] for why the first measurement is the one
    /// that decides.
    static LABEL_COST: RefCell<BTreeMap<String, usize>> = const { RefCell::new(BTreeMap::new()) };
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
    previous_label_costs: BTreeMap<String, usize>,
}

impl AbbrBudgetGuard {
    /// Install the budget for one render of `doc`.
    ///
    /// Every renderer sizes its budget through this one call, so the document's
    /// length is read in exactly ONE place. That matters because the number is
    /// not always measured: on the AST-ingest path `source_len` arrives inside
    /// the payload (`srcByteLength` on the wire), where a hostile tree can
    /// inflate it to widen the guard that is meant to bound it. Whatever ends up
    /// bounding that claim binds every target at once from here, instead of
    /// having to find four spellings of the same read.
    pub(crate) fn for_document(doc: &crate::ast::Document) -> Self {
        let previous =
            REMAINING.with(|cell| cell.replace(Some(budget_for(doc.expansion_budget_len()))));
        let previous_label_costs =
            LABEL_COST.with(|costs| std::mem::take(&mut *costs.borrow_mut()));
        AbbrBudgetGuard {
            previous,
            previous_label_costs,
        }
    }
}

impl Drop for AbbrBudgetGuard {
    fn drop(&mut self) {
        REMAINING.with(|cell| cell.set(self.previous));
        LABEL_COST
            .with(|costs| *costs.borrow_mut() = std::mem::take(&mut self.previous_label_costs));
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

/// Does this target's label no longer fit what is left of the budget?
///
/// THE COST IS THE ONE MEASURED FIRST, re-tested against what remains. A later
/// render of the same label can come back CHEAPER than the first without having
/// become affordable, because the label's own bytes may come from an
/// abbreviation expansion - which charges this same budget and degrades to
/// nothing once the budget is gone. A charge of zero fits anything, so measuring
/// the degraded render accepted an EMPTY label where carve-js and carve-php both
/// write the authored target (carve-rs#2130). Remembering only that a charge had
/// failed could not see it: at those expansion sizes no charge ever fails.
///
/// `REMAINING` never grows within a render, so an answer of `true` stays true.
pub(crate) fn label_rejected(target: &str) -> bool {
    let remaining = REMAINING.with(|cell| cell.get().unwrap_or(ABBR_EXPANSION_BUDGET_BASE));
    LABEL_COST.with(|costs| {
        costs
            .borrow()
            .get(target)
            .is_some_and(|cost| *cost > remaining)
    })
}

pub(crate) fn try_spend_label(cost: usize, target: &str) -> bool {
    #[cfg(test)]
    MEASURED_LABEL_BYTES.with(|bytes| bytes.set(bytes.get().saturating_add(cost)));
    LABEL_COST.with(|costs| {
        costs.borrow_mut().entry(target.to_owned()).or_insert(cost);
    });
    try_spend(cost)
}

#[cfg(test)]
mod tests {
    use super::MEASURED_LABEL_BYTES;

    #[test]
    fn rejected_crossref_labels_stop_rendering_on_every_target() {
        let source = format!("# A{}\n\n{}\n", "!".repeat(9_999), "</#A> ".repeat(3_000));
        let doc = crate::parse(&source);
        for target in 0..4 {
            MEASURED_LABEL_BYTES.with(|bytes| bytes.set(0));
            let output = match target {
                0 => crate::to_html(&source),
                1 => crate::render_plain_text(&doc).unwrap(),
                2 => crate::render_markdown(&doc).unwrap(),
                _ => crate::render_ansi(&doc).unwrap(),
            };
            assert!(!output.is_empty());
            let measured = MEASURED_LABEL_BYTES.with(|bytes| bytes.get());
            // `>=`, not `>`: the charge that no longer fits is no longer
            // ATTEMPTED, because the cost a label was measured at is re-tested
            // against what remains before it is rendered again (carve-rs#2130).
            // The old reading counted the failing charge too and so landed one
            // label past the bound; landing ON it is the tighter outcome.
            assert!(
                measured >= 1_000_000,
                "target {target} did not exhaust the budget"
            );
            assert!(
                measured < 2_000_000,
                "target {target} rendered {measured} label bytes"
            );
        }
    }
}
