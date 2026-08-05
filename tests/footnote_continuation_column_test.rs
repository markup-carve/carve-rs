//! A footnote continuation's indent is a COLUMN claim, not a character count
//! (carve-rs#663, spec markup-carve/carve#796).
//!
//! PART 9 §16 asks a continuation line for >= 2 columns, and §24 C1 gives a tab
//! a column value: it advances to the next multiple of 4 from wherever it
//! starts. So a bare tab reaches column 4 and continues the note exactly as two
//! literal spaces do.
//!
//! The gates here counted whitespace CHARACTERS with `leading_ws`, so a
//! `<SPACE><TAB>` was two (accepted, correct only by accident) and a bare
//! `<TAB>` was one (rejected) - while `strip_leading_columns`, the dedent
//! immediately below them, was already column-based. The gate and the dedent
//! disagreed about what the indent meant.
//!
//! The failure is not cosmetic. A rejected continuation does not render with
//! different spacing: it LEAVES the note and becomes a top-level paragraph above
//! the reference, so the content moves out of the endnote into the document
//! body.
//!
//! All three engines had a different half of this wrong and no two agreed on the
//! pair - carve-rs and carve-js took `<SPACE><TAB>` and refused the bare tab,
//! carve-php did the reverse.

/// True when `more` stayed inside the note rather than escaping into the body.
fn continues(indent: &str, blank: bool) -> bool {
    let source = format!(
        "[^a]: note\n{}{}more\n\nsee[^a]\n",
        if blank { "\n" } else { "" },
        indent
    );
    let html = carve::to_html(&source);
    !html.contains("<p>more</p>\n<p>see") && html.contains("more")
}

#[test]
fn two_spaces_continue_the_note() {
    // The shape every engine already agreed on, here to prove the floor did not
    // move rather than that it widened.
    assert!(continues("  ", true));
}

#[test]
fn a_bare_tab_continues_the_note() {
    assert!(continues("\t", true));
}

#[test]
fn a_space_then_a_tab_continues_the_note() {
    assert!(continues(" \t", true));
}

#[test]
fn a_bare_tab_continues_the_note_with_no_blank_line() {
    // The blank-line lookahead is a second gate, and it counted characters too.
    assert!(continues("\t", false));
}

#[test]
fn one_space_still_falls_short() {
    // Column 1. This is what keeps the rule from being "any indent at all".
    assert!(!continues(" ", true));
}

#[test]
fn a_flush_left_line_still_ends_the_note() {
    assert!(!continues("", true));
}

#[test]
fn indent_past_the_body_column_is_residual_the_body_reads_itself() {
    // Two tabs reach column 8 against a body column of 2. The dedent
    // (`strip_leading_columns`) was already column-based, so the extra columns
    // come back as the body's own residual indent - which its blocks read
    // themselves, leaving one paragraph rather than anything re-read as a
    // nested block.
    let html = carve::to_html("[^a]: note\n\n\t\tmore\n\nsee[^a]\n");
    assert!(
        html.contains("<p>note</p>\n      <p>more"),
        "expected two paragraphs in the note body, got:\n{html}"
    );
    assert!(
        !html.contains("<pre"),
        "residual indent became a code block"
    );
}
