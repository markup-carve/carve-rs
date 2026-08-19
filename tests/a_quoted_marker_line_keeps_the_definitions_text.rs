//! A QUOTED marker line is lazy text like its flush-left twin, so the definition
//! on it stays on the page and defines nothing (carve-rs#1142).
//!
//! Both definition pre-passes scoped their lazy guard with a marker test over the
//! RAW line, so `> - [d]: u` matched no marker and never reached the probe. The
//! collection then went ahead: the `> - ` prefix came off, a definition was
//! recognised behind it, and its text was cut out of the quote's open paragraph -
//! leaving a bare `-` where the author wrote a definition, and a live symbol for
//! a line that renders as text.
//!
//! The fix takes the container prefixes off before the marker test.
//! `line_folds_into_an_open_paragraph` needed no change at all: the run it hands
//! the block parser carries the quote, so `>`, `:::` and the document level get
//! the same answer from the same code. That is the argument for asking the parser
//! rather than enumerating - the container never had to become a case.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

/// The ticket's document. The text was `<p>r\n-</p>` before - the definition
/// deleted, the marker left behind.
#[test]
fn the_definitions_text_survives_inside_the_quote() {
    assert_eq!(
        html("> r\n> - [d]: u\n\n[go][d]\n"),
        "<blockquote><p>r\n- [d]: u</p></blockquote>\n<p>[go][d]</p>"
    );
}

/// And it defines nothing, which is the half a text-only assertion would miss.
#[test]
fn the_reference_does_not_resolve_from_a_quoted_lazy_line() {
    let out = html("> r\n> - [d]: u\n\n[go][d]\n");
    assert!(out.contains("[go][d]"), "{out}");
    assert!(!out.contains("<a href=\"u\">"), "{out}");
}

/// The footnote kind answers the same way - one probe, two definition kinds.
#[test]
fn the_footnote_kind_answers_the_same_way() {
    let out = html("> r\n> - [^f]: t\n\n[^f] ref\n");
    assert_eq!(
        out,
        "<blockquote><p>r\n- [^f]: t</p></blockquote>\n<p>[^f] ref</p>"
    );
    assert!(!out.contains("doc-endnotes"), "{out}");
}

/// Every marker dialect, because the dialect changes numbering and nothing else.
#[test]
fn every_marker_dialect_is_lazy_text_in_a_quote() {
    assert_eq!(
        html("> r\n> - [d]: u\n"),
        "<blockquote><p>r\n- [d]: u</p></blockquote>"
    );
    assert_eq!(
        html("> r\n> . [d]: u\n"),
        "<blockquote><p>r\n. [d]: u</p></blockquote>"
    );
    assert_eq!(
        html("> r\n> 1. [d]: u\n"),
        "<blockquote><p>r\n1. [d]: u</p></blockquote>"
    );
}

/// A marker line ALREADY folded into the quoted paragraph keeps the next one
/// folded, the quoted twin of the flush-left row in the footnote suite: reading
/// only the previous line sees a marker there, concludes no paragraph is open,
/// and collects.
#[test]
fn a_quoted_marker_already_folded_keeps_the_next_one_folded() {
    assert_eq!(
        html("> r\n> - a\n> - [d]: u\n\n[go][d]\n"),
        "<blockquote><p>r\n- a\n- [d]: u</p></blockquote>\n<p>[go][d]</p>"
    );
}

// ---------------------------------------------------------------------------
// CONTROLS. No paragraph is open in any of these, so the marker opens a REAL
// item and its definition IS collected. Each one fails if the guard is widened
// past "a paragraph the marker line can continue".
// ---------------------------------------------------------------------------

#[test]
fn control_a_quoted_marker_with_nothing_above_it_collects() {
    assert_eq!(
        html("> - [d]: u\n\n[go][d]\n"),
        "<blockquote>\n  <ul>\n    <li></li>\n  </ul>\n</blockquote>\n<p><a href=\"u\">go</a></p>"
    );
}

#[test]
fn control_a_blank_quoted_line_closes_the_paragraph_and_collects() {
    assert_eq!(
        html("> r\n>\n> - [d]: u\n\n[go][d]\n"),
        "<blockquote>\n  <p>r</p>\n  <ul>\n    <li></li>\n  </ul>\n</blockquote>\n<p><a href=\"u\">go</a></p>"
    );
}

/// A quote INTERRUPTS a paragraph (§10), so a quoted marker under top-level prose
/// opens a real quote holding a real item - the paragraph the marker would have
/// continued is not the one above it. This row is why the probe has to answer
/// rather than the marker test: both lines carry a marker after the prefix strip,
/// and only the parser knows the quote came between them.
#[test]
fn control_top_level_prose_above_a_quoted_marker_collects() {
    assert_eq!(
        html("para\n> - [d]: u\n\n[go][d]\n"),
        "<p>para</p>\n<blockquote>\n  <ul>\n    <li></li>\n  </ul>\n</blockquote>\n<p><a href=\"u\">go</a></p>"
    );
}

/// An abbreviation definition is not a marker-line definition at all here - it
/// stays visible text in every engine, and this change must not start collecting
/// it.
#[test]
fn control_an_abbreviation_definition_is_untouched() {
    assert_eq!(
        html("> r\n> - *[AB]: x\n"),
        "<blockquote><p>r\n- *[AB]: x</p></blockquote>"
    );
}
