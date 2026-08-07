//! PART 5 / PART 9: a definition MARKER's separator is a space, and it is a RUN.
//!
//! ```text
//! footnote_definition     = "[^", footnote_label, "]:", space+, inline_content, ...
//! abbreviation_definition = "*[", abbreviation_term, "]:", space+, abbreviation_expansion, newline ;
//! ```
//!
//! Two halves, settled together on carve#892:
//!
//! 1. the separator is a literal SPACE, as it always was - a tab after the
//!    marker is not a separator, so `*[HTML]:<TAB>x` and `[^f]:<TAB>x` stay
//!    paragraphs;
//! 2. it is a RUN of ASCII spaces, so **the first character that is not one ends
//!    the separator and BEGINS the content**.
//!
//! The measurement that produced the ruling read this engine as CONSUMING that
//! character at the abbreviation marker and KEEPING it at the footnote marker.
//! No engine gave the same answer for both markers.
//!
//! The trap for whoever measures it: check the rendered CONTENT, not the
//! construct. All three engines define the footnote either way; the difference
//! is whether the note text starts with the character.
//!
//! Cardinality is per-POSITION, not global: carve#912 ruled the OPPOSITE way for
//! four PADDING slots, which take exactly one space. A padding slot sits between
//! two tokens on a line whose construct is already fixed; a marker separator is
//! what stands between the marker and the content it introduces.

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The abbreviation title as rendered, so the assertion is on CONTENT rather
/// than on whether a definition formed.
fn abbr_title(src: &str) -> String {
    let html = to_html(src);
    let start = html
        .find("title=\"")
        .unwrap_or_else(|| panic!("no abbr in: {html}"))
        + 7;
    let rest = &html[start..];
    rest[..rest.find('"').expect("closing quote")].to_string()
}

// ---------------------------------------------------------------------------
// The separator is a RUN
// ---------------------------------------------------------------------------

#[test]
fn a_two_space_run_is_one_separator_at_the_abbreviation_marker() {
    assert_eq!(abbr_title("*[HTML]:  Hyper Text\n\nHTML\n"), "Hyper Text");
}

#[test]
fn a_two_space_run_is_one_separator_at_the_footnote_marker() {
    let html = to_html("x[^f]\n\n[^f]:  note\n");
    assert!(html.contains("<p>note<a href=\"#fnref1\""), "{html}");
}

// ---------------------------------------------------------------------------
// The first character that is not a space BEGINS the content
// ---------------------------------------------------------------------------

#[test]
fn a_no_break_space_after_the_run_is_content_at_the_abbreviation_marker() {
    // The row this engine had wrong: the character is IN the title.
    assert_eq!(
        abbr_title("*[HTML]: \u{a0}Hyper Text\n\nHTML\n"),
        "\u{a0}Hyper Text"
    );
}

#[test]
fn a_no_break_space_after_the_run_is_content_at_the_footnote_marker() {
    let html = to_html("x[^f]\n\n[^f]: \u{a0}note\n");
    assert!(html.contains("<p>&nbsp;note<a href=\"#fnref1\""), "{html}");
}

#[test]
fn a_tab_after_the_run_is_content_at_the_abbreviation_marker() {
    // An `abbreviation_expansion` is a raw string, so the tab survives into the
    // title.
    assert_eq!(
        abbr_title("*[HTML]: \tHyper Text\n\nHTML\n"),
        "\tHyper Text"
    );
}

#[test]
fn a_tab_after_the_run_is_the_footnote_body_s_own_indentation() {
    // The two markers answer differently here for a reason DOWNSTREAM of the
    // separator rather than in it: a footnote's `inline_content` is parsed as
    // blocks, and a leading tab is that body's own indentation run (PART 9
    // §24 C1), so it does not appear in the body.
    let html = to_html("x[^f]\n\n[^f]: \tnote\n");
    assert!(html.contains("<p>note<a href=\"#fnref1\""), "{html}");
}

// ---------------------------------------------------------------------------
// CONTROLS - widening the RUN is not widening the TERMINAL
// ---------------------------------------------------------------------------

#[test]
fn control_a_tab_as_the_separator_is_not_a_separator_at_either_marker() {
    assert_eq!(
        squash(&to_html("*[HTML]:\tHyper Text\n\nHTML\n")),
        "<p>*[HTML]: Hyper Text</p> <p>HTML</p>"
    );
    assert_eq!(
        squash(&to_html("x[^f]\n\n[^f]:\tnote\n")),
        "<p>x[^f]</p> <p>[^f]: note</p>"
    );
}

#[test]
fn control_a_marker_with_nothing_after_it_is_a_paragraph() {
    // MARKER REQUIRES CONTENT still applies after the run: a patch that
    // implements the run as "eat spaces then take the rest" makes a
    // spaces-only line define an empty footnote.
    assert_eq!(
        squash(&to_html("x[^f]\n\n[^f]:\n")),
        "<p>x[^f]</p> <p>[^f]:</p>"
    );
    assert_eq!(
        squash(&to_html("x[^f]\n\n[^f]:   \n")),
        "<p>x[^f]</p> <p>[^f]:</p>"
    );
    assert_eq!(squash(&to_html("*[A]: \n")), "<p>*[A]:</p>");
    assert_eq!(squash(&to_html("*[A]:\n")), "<p>*[A]:</p>");
}

#[test]
fn control_the_one_space_form_is_untouched() {
    assert_eq!(abbr_title("*[HTML]: Hyper Text\n\nHTML\n"), "Hyper Text");
    let html = to_html("x[^f]\n\n[^f]: note\n");
    assert!(html.contains("<p>note<a href=\"#fnref1\""), "{html}");
}

#[test]
fn control_a_definition_marker_is_not_a_padding_slot() {
    // carve#912 narrowed four PADDING slots to exactly one space in the same
    // week. This asserts the two rules did not get crossed: the marker
    // separator still takes a run of three.
    assert_eq!(abbr_title("*[HTML]:   Hyper Text\n\nHTML\n"), "Hyper Text");
}
