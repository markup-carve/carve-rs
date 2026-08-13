//! The four semantic span names core does not reserve, plus the deprecated
//! `:name[…]` spelling for all seven (spec PART 9 §10, docs/extensions.md §11).
//!
//! Core reserves `abbr`, `time` and `kbd` as span attributes: the first two
//! carry data the author would otherwise lose, and the third is the one name
//! every comparable system ships. `samp`, `var`, `cite` and `dfn` carry no data
//! and collide with no core clause, so they are opt-in - a core processor
//! leaves them as ordinary attributes (`<span samp="">x</span>`).
//!
//! ```text
//! [x]{samp}                      ->  <samp>x</samp>
//! [CSS]{dfn="Cascading Style…"}  ->  <dfn title="Cascading Style…">CSS</dfn>
//! ```
//!
//! THE `:name[…]` SPELLING IS SOFT-DEPRECATED HERE, not revived. It was
//! released behavior in this crate and in carve-js, so removing it outright
//! would break documents that shipped; it is scheduled for removal in 0.2.
//! Write the span attribute instead - it is the only spelling that can express
//! a combination, since `:dfn[:abbr[CSS]]` does not nest while
//! `[CSS]{dfn abbr="…"}` does.
//!
//! The span half is DECLARATIVE: this names the four it claims and the core
//! renderer renders them, so the nesting order, the value mapping and the
//! riding rule have one implementation rather than two that drift.

use crate::extension::CarveExtension;

/// The four names core does not reserve.
pub const NAMES: [&str; 4] = ["samp", "var", "cite", "dfn"];

/// Registering this makes the four names behave exactly as core's three do,
/// and re-registers the deprecated `:name[…]` spelling for all seven.
pub struct SemanticSpan;

impl CarveExtension for SemanticSpan {
    fn name(&self) -> &'static str {
        "semantic-span"
    }

    fn semantic_span_names(&self) -> &'static [&'static str] {
        &NAMES
    }
}
