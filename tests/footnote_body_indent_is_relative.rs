//! A footnote body's ">= 2" is measured from the DEFINITION, not column 0
//! (PART 9 §16, carve-rs#591).
//!
//! Measured from column 0, an INDENTED definition swallowed anything at column
//! 2 - including the `:::` closer of the container it sits in, which then
//! rendered as an empty `<div>` inside the endnote and pushed the backlink out
//! of its paragraph.

use carve::to_html;

fn note_body(src: &str) -> String {
    let html = to_html(src);
    let start = html.find("<li id=\"fn1\">").expect("no endnote");
    let end = html[start..].find("</li>").expect("unterminated endnote") + start;
    html[start..end].to_string()
}

#[test]
fn a_container_closer_is_not_note_body() {
    let body = note_body("- a\n\n  ::: note\n  [^f]: x\n  :::\n\nsee[^f]\n");

    assert!(
        !body.contains("<div>"),
        "closer became note content: {body}"
    );
    assert!(
        body.contains("<p>x<a href=\"#fnref1\""),
        "unexpected body: {body}"
    );
}

#[test]
fn a_continuation_two_columns_past_the_definition_still_counts() {
    let body = note_body("- a\n\n  [^f]: x\n    more\n\nsee[^f]\n");

    assert!(body.contains("more"), "continuation dropped: {body}");
}

#[test]
fn a_line_at_the_definitions_own_column_does_not() {
    // carve-js and carve-php both stop here: relative, not absolute.
    let body = note_body("- a\n\n  [^f]: x\n  more\n\nsee[^f]\n");

    assert!(
        !body.contains("more"),
        "line wrongly folded into the note: {body}"
    );
}

#[test]
fn a_top_level_definition_is_unchanged() {
    let body = note_body("[^f]: x\n  more\n\nsee[^f]\n");

    assert!(body.contains("more"), "top-level continuation lost: {body}");
}
