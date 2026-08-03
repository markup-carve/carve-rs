//! An empty reference label is not a label, so `[]: u` is a paragraph.
//!
//! The grammar requires at least one character:
//!
//!     reference_label = (character - ']' - '@'), {character - ']'} ;
//!
//! This engine consumed `[]: u` as a definition and emitted NOTHING, so the
//! line disappeared from the document. carve-js and carve-php both keep it as
//! text (carve-rs#451).
//!
//! Found by differential fuzzing across the three engines, then shrunk - no
//! corpus document covers an empty label, which is why every gate was green.

#[test]
fn an_empty_reference_label_is_not_a_definition() {
    assert_eq!(carve::to_html("[]: u\n"), "<p>[]: u</p>");
}

#[test]
fn the_rest_of_the_document_is_unaffected() {
    // The failure mode was a line vanishing from the middle of a document, not
    // an empty render, so the neighbours are what make it visible.
    assert_eq!(
        carve::to_html("[]: u\n\nafter\n"),
        "<p>[]: u</p>\n<p>after</p>"
    );
}

#[test]
fn a_one_character_label_still_defines() {
    // The fix is about EMPTY, not short. A single character is a legal label,
    // so this must still resolve and contribute no output of its own.
    assert_eq!(
        carve::to_html("[a]: u\n\n[x][a]\n"),
        "<p><a href=\"u\">x</a></p>"
    );
}

#[test]
fn a_whitespace_label_still_defines() {
    // A space is a `character`, so `[ ]` is a legal one-character label. All
    // three engines already treat it as a definition; this pins that the fix
    // does not widen into it.
    assert_eq!(carve::to_html("[ ]: u\n\nafter\n"), "<p>after</p>");
}

#[test]
fn a_citation_definition_is_still_not_a_link_definition() {
    // The grammar excludes `@` from the first character; `[@k]: v` is a
    // citation definition (PART 9 §22), not a reference. Pinned so the empty
    // check does not disturb the other exclusion in the same production.
    let html = carve::to_html("[@k]: v\n");
    assert!(html.contains("@k"), "the line should survive, got: {html}");
}
