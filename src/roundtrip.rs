//! The PART 10 §13a HTML roundtrip carriers: one enumeration, read by the
//! writer and by the importer.
//!
//! §13a reserves a SET of names rather than a prefix pattern, so the set lives
//! here and nowhere else. Two of the names predate the prefix (`data-marker`,
//! `data-char`) and keep their own spelling, ruled on markup-carve/carve#2902
//! question 3.
//!
//! THE PREFIX IS `data-carve-`, ruled on carve#2902 question 0. The carriers
//! were named `data-djot-*` because Carve forked from Djot and carve-php kept
//! the attribute names; a writer now emits `data-carve-*` only, and a READER
//! accepts `data-djot-*` as a deprecated alias so a document exported by a
//! released carve-php keeps round-tripping.

/// The authored list marker, where it is not the default (`<ul>`, `<ol>`).
pub(crate) const MARKER: &str = "data-marker";
/// The authored thematic-break character, where it is not `-` (`<hr>`).
pub(crate) const CHAR: &str = "data-char";
/// Presence: the id was authored, not generated (`<section>`, `<h1>`-`<h6>`).
pub(crate) const EXPLICIT_ID: &str = "data-carve-explicit-id";
/// The reference label a link or image resolved by (`<a>`, `<img>`).
pub(crate) const REF: &str = "data-carve-ref";
/// Presence: the link was authored as an autolink (`<a>`).
pub(crate) const AUTOLINK: &str = "data-carve-autolink";
/// The authored footnote label, on the reference and on the endnote item.
pub(crate) const FOOTNOTE_LABEL: &str = "data-carve-footnote-label";
/// Presence: the endnote came from an inline footnote (`<li>`).
pub(crate) const INLINE_FOOTNOTE: &str = "data-carve-inline-footnote";
/// The largest separator width this CARRIER may spell (PART 10 §13a,
/// `CARVE-P10-016`).
///
/// The widths arrive in an attribute read off untrusted HTML and the Carve
/// writer spells one `-` per unit, so six characters of attribute would buy a
/// megabyte of separator row. A reader REFUSES the whole carrier past the
/// bound rather than clamping to it, and a writer writes no carrier at all for
/// a field above it: a clamp and a partial read both put a separator row on the
/// page whose widths nobody wrote, where refusing leaves the table correct in
/// its other spelling. The FIELD itself has no upper bound - a parse records
/// what the author typed, and nothing amplifies there. 1000 is a round number
/// pinned for cross-engine parity, not a bound derived from anything.
pub(crate) const SEPARATOR_WIDTH_LIMIT: u64 = 1000;

/// The authored fence label (`<pre>`).
pub(crate) const LABEL: &str = "data-carve-label";

/// The authored separator-row widths, comma-joined (`<table>`).
pub(crate) const COL_WIDTHS: &str = "data-carve-col-widths";
/// The raw inline's format; the span holds its content (`<span>`).
pub(crate) const RAW: &str = "data-carve-raw";
/// Presence; the span holds the escaped character (`<span>`).
pub(crate) const ESCAPED: &str = "data-carve-escaped";
/// Presence: the heading id is generated, on its wrapper or heading.
pub(crate) const GENERATED_ID: &str = "data-carve-generated-id";
/// The authored id's index in the heading's `attrs.order`, omitted at 0
/// (`<section>`, beside [`EXPLICIT_ID`]).
pub(crate) const ID_SLOT: &str = "data-carve-id-slot";
/// Presence: the list is tight despite an attributed item paragraph.
pub(crate) const TIGHT: &str = "data-carve-tight";
/// Reserved without a row: no writer emits it, every importer consumes it, and
/// roundtrip export drops an authored one.
pub(crate) const SRC: &str = "data-carve-src";
/// The document's abbreviation definitions, one per line (`<template>`).
pub(crate) const ABBREVIATIONS: &str = "data-carve-abbreviations";

/// Presence: a resolved cross-reference anchor.
pub(crate) const XREF: &str = "data-carve-xref";
/// Presence: a caption number's text-only span.
pub(crate) const CAPTION_NUMBER: &str = "data-carve-caption-number";
/// Presence: a `div` node with no attribute of its own.
pub(crate) const DIV: &str = "data-carve-div";
/// Presence: a `line_block` node, which `class="line-block"` alone does not
/// tell from an admonition of that kind.
pub(crate) const LINE_BLOCK: &str = "data-carve-line-block";
/// Presence: the span wrapping a substitution's `<del>` and `<ins>`.
pub(crate) const SUBSTITUTION: &str = "data-carve-substitution";

/// Every name a reader consumes: the §13a inventory plus [`SRC`], which no writer emits.
pub(crate) const CARRIERS: [&str; 21] = [
    MARKER,
    CHAR,
    EXPLICIT_ID,
    REF,
    AUTOLINK,
    FOOTNOTE_LABEL,
    INLINE_FOOTNOTE,
    COL_WIDTHS,
    LABEL,
    RAW,
    ESCAPED,
    GENERATED_ID,
    ID_SLOT,
    TIGHT,
    SRC,
    ABBREVIATIONS,
    XREF,
    CAPTION_NUMBER,
    DIV,
    LINE_BLOCK,
    SUBSTITUTION,
];

/// The canonical §13a name an attribute spells, or `None` for a name outside
/// the inventory.
///
/// Both spellings answer: `data-djot-ref` and `data-carve-ref` map to
/// [`REF`]. The comparison is ASCII-case-insensitive because an HTML attribute
/// name is.
pub(crate) fn canonical(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    if let Some(found) = CARRIERS.iter().find(|c| **c == lower) {
        return Some(found);
    }
    let rest = lower.strip_prefix("data-djot-")?;
    CARRIERS
        .iter()
        .copied()
        .find(|c| c.strip_prefix("data-carve-") == Some(rest))
}

/// Report reserved authored names before rendering without changing layout.
pub(crate) fn report_reserved_attributes(doc: &mut crate::ast::Document) {
    use crate::include_walk::SubtreeVisitor;
    #[derive(Default)]
    struct Reporter {
        pos: Option<crate::ast::Pos>,
    }
    impl SubtreeVisitor for Reporter {
        fn position_slot(&mut self, pos: &mut Option<crate::ast::Pos>) {
            self.pos = pos.clone();
        }
        fn attributes(&mut self, attrs: &mut Option<crate::ast::Attrs>, inline: bool) {
            let Some(held) = attrs else { return };
            let mut names = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for slot in &held.order {
                if let crate::ast::AttrSlot::Key(name) = slot {
                    if held.key_values.contains_key(name) && seen.insert(name.as_str()) {
                        names.push(name.as_str());
                    }
                }
            }
            for name in held.key_values.keys() {
                if seen.insert(name.as_str()) {
                    names.push(name.as_str());
                }
            }
            for name in names {
                if let Some(carrier) = canonical(name) {
                    crate::render_loss::record_carrier_collision(
                        carrier,
                        name,
                        self.pos.as_ref(),
                        if inline {
                            crate::render_loss::RawNodeType::Inline
                        } else {
                            crate::render_loss::RawNodeType::Block
                        },
                    );
                }
            }
        }
        fn blocks(&mut self, blocks: &mut Vec<crate::ast::BlockNode>) {
            for block in blocks {
                crate::include_walk::visit_block_children(block, self);
            }
        }
        fn inlines(&mut self, inlines: &mut Vec<crate::ast::InlineNode>) {
            for node in inlines {
                crate::include_walk::visit_inline_children(node, self);
            }
        }
    }
    let mut reporter = Reporter::default();
    reporter.blocks(&mut doc.children);
    for blocks in doc.footnote_defs.values_mut() {
        reporter.blocks(blocks);
    }
}
