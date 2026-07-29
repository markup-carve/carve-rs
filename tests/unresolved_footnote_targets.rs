//! An unresolved footnote reference is not a footnote, in the ANSI and Markdown
//! targets as well as in plain (carve-rs#311).
//!
//! The HTML renderer tells resolved from unresolved via the node's `number`,
//! which numbering assigns. None of these targets numbers anything, so that
//! field is permanently None in all three and each had a decision point with
//! nothing to decide on -- which is why fixing one did nothing for the others
//! (carve#352, corpus 132/133/157/161).

const ESC: char = '\u{1b}';

#[test]
fn ansi_leaves_an_unresolved_reference_literal_and_unstyled() {
    let out = carve::to_ansi("Use [^a].\n");
    assert_eq!(out, "Use [^a].\n");
    assert!(!out.contains(ESC), "styling was applied: {out:?}");
}

#[test]
fn ansi_still_styles_a_resolved_reference() {
    let out = carve::to_ansi("Use [^a].\n\n[^a]: A real note.\n");
    assert!(out.contains("[a]"), "marker missing: {out:?}");
    assert!(out.contains(ESC), "styling missing: {out:?}");
}

#[test]
fn markdown_escapes_an_unresolved_reference() {
    // The brackets are Markdown metacharacters; PART 11 section 8 M1 escapes them
    // unconditionally, so a GFM processor cannot read a reference that is not
    // there.
    assert_eq!(carve::to_markdown("Use [^a].\n"), "Use \\[^a\\].\n");
}

#[test]
fn markdown_keeps_a_resolved_reference_as_a_real_footnote() {
    assert_eq!(
        carve::to_markdown("Use [^a].\n\n[^a]: A real note.\n"),
        "Use [^a].\n\n[^a]: A real note.\n"
    );
}

#[test]
fn a_definition_for_another_label_does_not_resolve_it() {
    assert!(carve::to_ansi("Use [^a].\n\n[^b]: Other.\n").contains("[^a]"));
    assert!(carve::to_markdown("Use [^a].\n\n[^b]: Other.\n").contains("\\[^a\\]"));
}

#[test]
fn an_inline_note_is_unaffected() {
    assert_eq!(carve::to_markdown("Use ^[a note].\n"), "Use ^[a note].\n");
    assert!(carve::to_ansi("Use ^[a note].\n").contains("(a note)"));
}
