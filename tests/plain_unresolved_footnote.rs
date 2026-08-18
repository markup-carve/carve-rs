//! A footnote reference with no matching definition did not form a footnote, so
//! every target has to reproduce it as source text.
//!
//! The HTML renderer decides that on the node's `number`, which numbering
//! assigns. The plain target does no numbering, so the field is always None
//! there and every reference rendered the same way -- dropping the caret and
//! inventing a reference the document does not have. carve-php was the only
//! engine getting this right (carve#352, corpus 132/133/157/161).
//!
//! The DEFINITION lines below carry their caret for a different rule:
//! PART 11 §10a emits the marker as written, and `[a]: …` is a link
//! reference definition rather than this construct. Whether the
//! REFERENCE marker should keep its caret on this target is carve#550,
//! still open, so these pin it as it stands.

#[test]
fn an_unresolved_reference_keeps_its_caret() {
    assert_eq!(carve::to_plain_text("Use [^a].\n"), "Use [^a].\n");
}

#[test]
fn it_agrees_with_the_html_target_about_the_same_input() {
    // HTML renders the unresolved reference as literal source; plain must not
    // disagree about whether the construct exists.
    assert!(carve::to_html("Use [^a].\n").contains("[^a]"));
    assert!(carve::to_plain_text("Use [^a].\n").contains("[^a]"));
}

#[test]
fn a_resolved_reference_still_renders_as_a_marker() {
    assert_eq!(
        carve::to_plain_text("Use [^a].\n\n[^a]: A real note.\n"),
        "Use [a].\n\n[^a]: A real note.\n"
    );
}

#[test]
fn a_definition_for_another_label_does_not_resolve_it() {
    assert_eq!(
        carve::to_plain_text("Use [^a].\n\n[^b]: Other.\n"),
        "Use [^a].\n\n[^b]: Other.\n"
    );
}

#[test]
fn an_inline_note_is_unaffected() {
    assert_eq!(carve::to_plain_text("Use ^[a note].\n"), "Use (a note).\n");
}

#[test]
fn the_label_set_does_not_leak_between_renders() {
    // The set lives in a thread-local, so a document WITH a definition must not
    // make a later document without one resolve.
    assert_eq!(
        carve::to_plain_text("Use [^a].\n\n[^a]: A real note.\n"),
        "Use [a].\n\n[^a]: A real note.\n"
    );
    assert_eq!(carve::to_plain_text("Use [^a].\n"), "Use [^a].\n");
}
