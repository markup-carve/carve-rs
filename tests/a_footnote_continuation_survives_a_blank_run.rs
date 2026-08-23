//! An indented continuation after a run of blank lines stays INSIDE the footnote
//! definition (markup-carve/carve#1620).
//!
//! The body's blank-line test read only the line after the blank, so at TWO
//! blanks the line it looked at was itself blank -- neither indented nor a `+`
//! marker -- and the definition ended. The continuation was then ejected to a
//! top-level paragraph, and not to where it was written either: it landed ahead
//! of the endnotes section, so the content moved backward past unrelated blocks.
//!
//! A blank run does not end an indented block anywhere else in Carve. A list
//! item, a quote and a container all keep an indented continuation across one,
//! and nothing in PART 9 §16 says a footnote definition differs. carve-js
//! already read it this way at every count.
//!
//! NOT PART 9 §11 N1a. That fires at three or more blank lines and only before a
//! LIST MARKER. This fired at two, and for a plain paragraph as readily as for a
//! list, so the hard boundary settled in markup-carve/carve#1430 is untouched --
//! and the case at the bottom shows it still firing inside a note body.

/// The note's rendered list item, so a case can ask what ended up inside it.
fn note_body(source: &str) -> String {
    let html = carve::render_html(&carve::parse(source)).unwrap();
    let start = html.find("<li id=\"fn1\">").expect("no footnote rendered");
    html[start..].to_string()
}

fn source_with_blanks(count: usize) -> String {
    format!("See[^1].\n\n[^1]: a\n{}    b\n", "\n".repeat(count))
}

/// The ticket's repro. One blank always worked; two is where it broke.
#[test]
fn a_continuation_after_two_blank_lines_stays_in_the_note() {
    let html = carve::render_html(&carve::parse(&source_with_blanks(2))).unwrap();
    assert!(note_body(&source_with_blanks(2)).contains("<p>b"), "{html}");
    assert!(
        !html.contains("<p>b</p>\n<section"),
        "b was ejected ahead of the endnotes: {html}"
    );
}

/// Every count reads the same, which is the actual rule: a blank RUN does not
/// end the definition. Three and four are included because the ruling asked
/// whether the same engines eject at three or more for a different reason -- they
/// did not, it was this one defect at every count above one.
#[test]
fn every_blank_run_length_keeps_the_continuation_in_the_note() {
    for count in 1..=4 {
        let source = source_with_blanks(count);
        assert!(
            note_body(&source).contains("<p>b"),
            "{count} blank line(s) ejected the continuation"
        );
    }
}

/// THE BOUND. A run of blanks keeps the body open only for a line that could
/// continue it. A flush-left line still ends the definition, at two blanks
/// exactly as at one.
#[test]
fn a_flush_left_line_after_a_blank_run_still_ends_the_definition() {
    let source = "See[^1].\n\n[^1]: a\n\n\nb\n";
    let html = carve::render_html(&carve::parse(source)).unwrap();
    assert!(html.contains("<p>b</p>"), "{html}");
    assert!(!note_body(source).contains("<p>b"), "{html}");
}

/// The blank run is pushed through intact rather than collapsed, so a genuine
/// §11 N1a boundary written INSIDE a note still reaches the parser as the author
/// wrote it: three blanks between two sub-lists keep them two lists.
#[test]
fn a_hard_list_boundary_inside_a_note_body_still_fires() {
    let source = "See[^1].\n\n[^1]: - a\n\n\n\n    - b\n";
    assert_eq!(note_body(source).matches("<ul>").count(), 2);
}
