//! Regression (carve-rs#2097): `fmt` replaced a list item with `+` whenever the
//! item's rendered content started with `[^` and held a colon-space, on the
//! theory that the item spelled a footnote definition collection had hoisted.
//! Collection always empties the item, so that text-shaped test could only fire
//! on authored inline content, and it deleted the item's whole text.
//!
//! The assertions compare against the INPUT BYTES, not against a round trip: a
//! round trip only reports what survived, which is what let the loss through.

/// The source-to-source path `carve fmt` and `carve --carve` both take.
fn fmt(src: &str) -> String {
    carve::to_carve(src)
}

/// The spellings from the ticket, each of which came back as `- +`.
#[test]
fn an_item_whose_text_opens_with_a_note_reference_comes_back_verbatim() {
    for src in [
        "* [^f]:: e\n",
        "- [^x]:: term\n",
        ". [^f]:: -\n",
        // The same loss, wider than the ticket: any colon-space in an item that
        // opens with a note reference used to empty it.
        "- [^f] and: x\n",
        "- [^f]\\: x\n",
        "- [^note] a: b\n",
    ] {
        assert_eq!(fmt(src), src, "fmt did not write the authored bytes back");
    }
}

/// Controls for an over-broad fix: none of these ever lost its text, and the
/// first three are one edit away from the losing spellings.
#[test]
fn the_neighbouring_spellings_are_untouched() {
    for src in [
        "- [x]:: term\n",
        "- a [^f]:: e\n",
        "[^f]:: e\n",
        "- [^f] x\n",
    ] {
        assert_eq!(fmt(src), src, "fmt changed a spelling that always survived");
    }
}

/// The case the deleted test was reaching for. A definition on the marker line
/// IS collected, so the item is genuinely empty and `+` is correct - and the
/// definition must be written once, at the document level, not inside the item.
#[test]
fn a_definition_on_the_marker_line_still_empties_its_item() {
    assert_eq!(fmt("- [^f]: note\n"), "- +\n\n[^f]: note\n");
    assert_eq!(fmt("> - [^f]: note\n"), "> - +\n\n[^f]: note\n");
    // Nested, the definition goes back on the line it was written on.
    assert_eq!(fmt("- - [^f]: note\n"), "- - [^f]: note\n");
}
