//! A TRAILING LINE AFTER A CONSUMED DEFINITION IN A STACK OF NESTED NOTES
//! (markup-carve/carve-php#1895, ruled canonical in markup-carve/carve#1946,
//! pinned by the corpus section added in markup-carve/carve#1959).
//!
//! The stack is three footnote definitions, each a shallower marker by one
//! note, with a link definition and a trailing word below the innermost. The
//! link definition is consumed (so `[t][r]` resolves) and the innermost `[^h]`
//! is a real third note. The open question is which note owns the trailing line.
//!
//! A note's body column is measured from its OWN marker - two columns past it,
//! not two past the frame's zero (carve-js#1664). A note nested one column shy
//! of its host's body column keeps a residual marker indent, so a trailing line
//! that does not reach the inner note's own content column is NOT claimed by it
//! and falls to the reachable ancestor note. This engine flattened every nested
//! note to its body's column zero, which collapsed the two-level dedent so the
//! innermost note over-reached and took a line that belongs to an outer note.
//!
//! ORACLE: carve-js main (#1664), which measures the per-marker body column and
//! matches the pinned carve#1959 goldens byte-for-byte across the disputed
//! matrix.

use carve::to_html;

// Marker columns: outer at 0, mid at `m`, inner at `i`; the link definition and
// the trailing word both at `p`. The reference line resolves `[r]` and cites
// all three notes in order, so fn1 = outer, fn2 = mid, fn3 = inner.
fn doc(m: usize, i: usize, p: usize) -> String {
    let sp = |n: usize| " ".repeat(n);
    [
        "[^f]: outer".to_string(),
        String::new(),
        format!("{}[^g]: mid", sp(m)),
        String::new(),
        format!("{}[^h]: inner", sp(i)),
        String::new(),
        format!("{}[r]: /url", sp(p)),
        format!("{}TAILWORD", sp(p)),
        String::new(),
        "x[^f] [^g] [^h] [t][r]".to_string(),
    ]
    .join("\n")
        + "\n"
}

fn note_count(html: &str) -> usize {
    html.matches("<li id=\"fn").count()
}

/// Which note holds the trailing word, read off the list item that contains it;
/// `Document` when it sits outside every note.
#[derive(Debug, PartialEq, Eq)]
enum Owner {
    Outer,
    Mid,
    Inner,
    Document,
}

fn tail_word_note(html: &str) -> Owner {
    let mut rest = html;
    while let Some(start) = rest.find("<li id=\"fn") {
        let body_start = rest[start..].find('>').map(|o| start + o + 1).unwrap();
        let end = rest[body_start..]
            .find("</li>")
            .map(|o| body_start + o)
            .unwrap_or(rest.len());
        let body = &rest[body_start..end];
        if body.contains("TAILWORD") {
            if body.contains("outer") {
                return Owner::Outer;
            }
            if body.contains("mid") {
                return Owner::Mid;
            }
            if body.contains("inner") {
                return Owner::Inner;
            }
        }
        rest = &rest[end..];
    }
    Owner::Document
}

fn reference_resolves(html: &str) -> bool {
    html.contains("<a href=\"/url\">t</a>")
}

/// The four degenerate `i = m + 1`, `p = i + 1` cells. The inner marker sits one
/// column shy of the mid note's body column, so the inner note keeps a residual
/// indent and the payload - which reaches no note's own content column past the
/// outer - falls to the outer note. This engine placed all four in the inner
/// note before the fix.
#[test]
fn a_degenerate_stack_leaves_the_trailing_line_with_the_outer_note() {
    for (m, i, p) in [(3, 4, 5), (4, 5, 6), (5, 6, 7), (6, 7, 8)] {
        let html = to_html(&doc(m, i, p));
        assert_eq!(note_count(&html), 3, "three notes on ({m},{i},{p})");
        assert_eq!(
            tail_word_note(&html),
            Owner::Outer,
            "the trailing line belongs to the outer note on ({m},{i},{p})"
        );
    }
}

/// The `m = 2` mid band: the payload reaches the mid note's body column but not
/// the inner note's own content column, so the mid note owns it. This engine
/// collapsed the band into the inner note before the fix.
#[test]
fn a_mid_band_payload_lands_in_the_mid_note() {
    let html = to_html(&doc(2, 4, 4));
    assert_eq!(note_count(&html), 3);
    assert!(reference_resolves(&html));
    assert_eq!(tail_word_note(&html), Owner::Mid);
}

/// A payload that DOES reach the inner note's own content column still lands in
/// the inner note - the fix narrows the over-reach, it does not close the note.
#[test]
fn a_canonical_payload_still_lands_in_the_inner_note() {
    let html = to_html(&doc(2, 4, 6));
    assert_eq!(note_count(&html), 3);
    assert!(reference_resolves(&html));
    assert_eq!(tail_word_note(&html), Owner::Inner);
}

/// The `Bo <= p < Bm` band: the payload reaches the outer note's body column but
/// not the mid note's, so the outer note owns it.
#[test]
fn a_payload_below_the_mid_column_lands_in_the_outer_note() {
    let html = to_html(&doc(2, 4, 3));
    assert_eq!(note_count(&html), 3);
    assert!(reference_resolves(&html));
    assert_eq!(tail_word_note(&html), Owner::Outer);
}

/// A payload below every note's body column is document text. The link
/// definition at column zero is still consumed, so the reference resolves.
#[test]
fn a_payload_below_every_note_is_document_text() {
    let html = to_html(&doc(2, 4, 0));
    assert_eq!(note_count(&html), 3);
    assert!(reference_resolves(&html));
    assert_eq!(tail_word_note(&html), Owner::Document);
    assert!(html.contains("<p>TAILWORD</p>"));
}
