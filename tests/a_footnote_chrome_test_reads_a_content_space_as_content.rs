//! markup-carve/carve-rs#1345. The three footnote-detach chrome tests read
//! `str::trim` too, so a content space in any of three slots around a footnote
//! was DELETED - and reading a content space as chrome is a deletion, not a
//! formatting difference.
//!
//! `str::trim` is `char::is_whitespace`, which is Unicode `White_Space` and holds
//! NO-BREAK SPACE (U+00A0), NARROW NO-BREAK SPACE (U+202F) and IDEOGRAPHIC SPACE
//! (U+3000). markup-carve/carve#1628 puts all three on the CONTENT side of the
//! line, verified empirically. `is_layout_space` in `src/html_import.rs` states
//! the set correctly; every site that hand-rolls a trim re-decides the question
//! without it, and gets it wrong.
//!
//! Split out of carve-rs#1342, which fixed the raw-DOM blank tests and NAMED
//! these three without measuring them. They are measured here, one slot at a
//! time, and all three were real: none was unreachable.
//!
//! ## What was wrong, measured on `main` at `3cbeca88`
//!
//! ```text
//! a content space immediately before the footnote separator
//!   before  "x[^1] y\n\n[^1]: note\n"                gone
//!   after   "x[^1] y\n\n\u{a0}\n\n[^1]: note\n"      kept
//!
//! a <sup> in the NOTE holding the backlink and a content space
//!   before  "[^1]: note"            the <sup> detached, the character with it
//!   after   "[^1]: note{^\u{a0}^}"  kept, exactly as a word there is
//!
//! a <sup> around the REFERENCE holding a content space beside the anchor
//!   before  "x[^1] y"               the <sup> taken as the reference site
//!   after   "x{^\u{a0}[^1]^} y"     kept, exactly as a word there is
//! ```
//!
//! ## The controls, which are the point
//!
//! An ordinary space in any of the three slots IS chrome and its treatment must
//! not move - a fix that widened the predicate instead of correcting it would
//! start keeping the margins every producer emits, which is the opposite defect
//! and just as invisible. So every case below carries three rows: the content
//! space, its layout twin, and an ordinary word. The word row is what says the
//! slot is reachable at all; the layout row is what says the fix did not
//! over-correct.

use carve::{html_to_carve, HtmlImportOptions};

const NBSP: &str = "\u{a0}";
const NNBSP: &str = "\u{202f}";
const IDEOGRAPHIC: &str = "\u{3000}";

fn carve(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

/// The reference, the separator and the note, with three slots to fill.
fn document(reference: &str, separator: &str, backlink: &str) -> String {
    format!(
        "<p>x{reference} y</p>\n\
         <section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n\
         {separator}<ol>\n<li id=\"fn1\">\n<p>note{backlink}</p>\n</li>\n</ol>\n</section>\n"
    )
}

const REF: &str = "<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\">1</a>";
const BACK: &str = "<a href=\"#fnref1\" role=\"doc-backlink\">\u{21a9}</a>";

// -- SLOT 1: is_footnote_chrome_node, the separator walk -------------------

#[test]
fn a_content_space_before_the_separator_is_kept() {
    for space in [NBSP, NNBSP, IDEOGRAPHIC] {
        let written = carve(&document(REF, &format!("{space}<hr>\n"), BACK));
        assert!(
            written.contains(space),
            "{space:?} before the separator was read as chrome and deleted: {written:?}"
        );
    }
}

#[test]
fn a_layout_space_before_the_separator_is_still_chrome() {
    // The control the fix must not break. A margin there is what every producer
    // measured actually emits, so keeping it would put a stray blank paragraph
    // into every imported document with footnotes.
    for space in [" ", "\t", "\n", "  \n  "] {
        let written = carve(&document(REF, &format!("{space}<hr>\n"), BACK));
        assert_eq!(
            written, "x[^1] y\n\n[^1]: note\n",
            "layout {space:?} before the separator stopped being chrome"
        );
    }
}

#[test]
fn a_word_before_the_separator_is_kept_which_is_why_the_slot_is_reachable() {
    let written = carve(&document(REF, "Z<hr>\n", BACK));
    assert!(written.contains('Z'), "{written:?}");
}

// -- SLOT 2: the `emptied` test in strip_footnote_backlinks ----------------

#[test]
fn a_content_space_beside_the_backlink_keeps_its_wrapper() {
    for space in [NBSP, NNBSP, IDEOGRAPHIC] {
        let written = carve(&document(
            REF,
            "<hr>\n",
            &format!("<sup>{space}{BACK}</sup>"),
        ));
        assert!(
            written.contains(space),
            "{space:?} beside the backlink was read as emptied and detached: {written:?}"
        );
    }
}

#[test]
fn a_layout_space_beside_the_backlink_still_empties_its_wrapper() {
    for space in [" ", "\t", "\n"] {
        let written = carve(&document(
            REF,
            "<hr>\n",
            &format!("<sup>{space}{BACK}</sup>"),
        ));
        assert_eq!(
            written, "x[^1] y\n\n[^1]: note\n",
            "layout {space:?} beside the backlink stopped emptying the wrapper"
        );
    }
}

#[test]
fn a_word_beside_the_backlink_keeps_its_wrapper() {
    let written = carve(&document(REF, "<hr>\n", &format!("<sup>Z{BACK}</sup>")));
    assert_eq!(written, "x[^1] y\n\n[^1]: note{^Z^}\n");
}

// -- SLOT 3: footnote_reference_site ---------------------------------------

#[test]
fn a_content_space_beside_the_reference_keeps_its_wrapper() {
    for space in [NBSP, NNBSP, IDEOGRAPHIC] {
        let written = carve(&document(
            &format!("<sup>{space}{REF}</sup>"),
            "<hr>\n",
            BACK,
        ));
        assert!(
            written.contains(space),
            "{space:?} beside the reference was swallowed with the wrapper: {written:?}"
        );
    }
}

#[test]
fn a_layout_space_beside_the_reference_still_yields_its_wrapper() {
    for space in [" ", "\t", "\n"] {
        let written = carve(&document(
            &format!("<sup>{space}{REF}</sup>"),
            "<hr>\n",
            BACK,
        ));
        assert_eq!(
            written, "x[^1] y\n\n[^1]: note\n",
            "layout {space:?} beside the reference stopped yielding the wrapper"
        );
    }
}

#[test]
fn a_word_beside_the_reference_keeps_its_wrapper() {
    let written = carve(&document(&format!("<sup>Z{REF}</sup>"), "<hr>\n", BACK));
    assert_eq!(written, "x{^Z[^1]^} y\n\n[^1]: note\n");
}
