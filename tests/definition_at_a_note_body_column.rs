//! A definition at a footnote body's own content column is collected
//! (PART 9 §7, carve-rs#599, markup-carve/carve#669).
//!
//! A note body is a container, and a definition inside a container is
//! collected - the rule block quotes and list items both settled this week. The
//! note body was the remaining kind, and this engine rendered the line as note
//! TEXT while the executable spec, carve-js and carve-php all collected it.
//!
//! ONLY at the body's own column. One column deeper the line is below the
//! body's column 0, where §24 C3 folds it as text - which this engine and the
//! oracle already do. markup-carve/carve#664 is open on the indents around the
//! continuation column; this is not that. Indent 2 is a genuine continuation,
//! the other three already agree, and it needs no decision.
//!
//! The whole indent sweep, measured against carve-js, carve-php and the
//! executable spec (V = the line renders, A = a reference resolves):
//!
//! ```text
//! indent  carve-rs (before)  carve-rs (after)  oracle  carve-js  carve-php
//!   0           -A                 -A            -A       -A        -A
//!   1           V-                 V-            -A       VA        VA
//!   2           V-                 -A            -A       -A        -A
//!   3           V-                 V-            V-       -A        VA
//! ```
//!
//! Rows 0, 2 and 3 now match the oracle. Row 1 is the open question.

use carve::to_html;

const NOTE_AND_LINK: &str = "[^a]: note\n  [r]: /u\n\nsee[^a] and [t][r]\n";

#[test]
fn a_definition_at_the_body_column_is_collected() {
    let out = to_html(NOTE_AND_LINK);
    assert!(
        out.contains("href=\"/u\""),
        "the reference did not resolve: {out}"
    );
    assert!(
        !out.contains("[r]: /u"),
        "the definition line rendered as text: {out}"
    );
}

#[test]
fn the_note_keeps_only_its_own_text() {
    // The other half of "collected": the line leaves the note body too, rather
    // than being registered AND kept. Both at once is the combination that is
    // defensible under no reading.
    let out = to_html(NOTE_AND_LINK);
    assert!(
        out.contains("<p>note<a href=\"#fnref1\""),
        "note body: {out}"
    );
}

#[test]
fn one_column_deeper_still_folds_as_text() {
    // The control against widening the rule. Below the body's own column 0 a
    // definition is text (§24 C3), which is what the oracle does here too -
    // and what markup-carve/carve#664 is left open to reconsider. A `>=` test
    // instead of `==` would trade a fixed divergence for a new one.
    let out = to_html("[^a]: note\n   [r]: /u\n\nsee[^a] and [t][r]\n");
    assert!(out.contains("[r]: /u"), "expected the line as text: {out}");
    assert!(!out.contains("href=\"/u\""), "it should not resolve: {out}");
}

#[test]
fn one_column_shallower_is_outside_the_body() {
    // The other side of the same control. One space does not reach the
    // continuation column, so the line is not in the note at all. This engine
    // renders it and does not register it; the oracle collects it. That
    // disagreement is markup-carve/carve#667 and is deliberately untouched -
    // pinned so a repair of it is a decision rather than a side effect.
    let out = to_html("[^a]: note\n [r]: /u\n\nsee[^a] and [t][r]\n");
    assert!(out.contains("[r]: /u"), "expected the line as text: {out}");
    assert!(!out.contains("href=\"/u\""), "it should not resolve: {out}");
}

#[test]
fn a_definition_shaped_line_with_no_destination_stays_in_the_note() {
    // Definition-SHAPED is not the same as a definition. The link pass this
    // hands the line to requires a non-empty destination and keeps the line as
    // literal text without one, so extracting on shape alone moved `[r]:` out
    // of the note and into a document-level paragraph. That relocates VISIBLE
    // content across a container boundary, which is worse than the bug this
    // file fixes. carve-js, carve-php and the oracle all keep it in the note.
    let out = to_html("[^a]: note\n  [r]: \n\nsee[^a]\n");
    assert!(
        !out.contains("<p>[r]:</p>"),
        "the line escaped the note as a document-level paragraph: {out}"
    );
    assert!(out.contains("[r]:"), "the line was lost entirely: {out}");
}

#[test]
fn a_flush_definition_after_the_note_is_unaffected() {
    // The shape that always worked: the body has closed, so this is an
    // ordinary document-level definition.
    let out = to_html("[^a]: note\n\n[r]: /u\n\nsee[^a] and [t][r]\n");
    assert!(out.contains("href=\"/u\""), "did not resolve: {out}");
    assert!(!out.contains("[r]: /u"), "leaked as text: {out}");
}

#[test]
fn ordinary_continuation_text_stays_in_the_note() {
    // Only DEFINITIONS are taken out of the body. A plain continuation line at
    // the same column is note content and must stay there.
    let out = to_html("[^a]: note\n  more\n\nsee[^a]\n");
    assert!(out.contains("more"), "continuation text lost: {out}");
}

#[test]
fn a_definition_inside_the_note_serves_the_note_too() {
    // Collected means collected at DOCUMENT level, so a reference from inside
    // the note body resolves against it as well as one from outside.
    let out = to_html("[^a]: note [t][r]\n  [r]: /u\n\nsee[^a]\n");
    assert!(
        out.contains("href=\"/u\""),
        "the note's own reference did not resolve: {out}"
    );
}
