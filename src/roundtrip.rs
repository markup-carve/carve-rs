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
/// The construct's Carve source verbatim (`<pre>`, a container).
pub(crate) const SRC: &str = "data-carve-src";
/// The document's abbreviation definitions, one per line (`<template>`).
pub(crate) const ABBREVIATIONS: &str = "data-carve-abbreviations";

/// Every §13a name in inventory order, canonically spelled.
pub(crate) const CARRIERS: [&str; 13] = [
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
    SRC,
    ABBREVIATIONS,
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
