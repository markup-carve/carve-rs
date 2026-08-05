//! A footnote continuation's indent is a COLUMN claim, not a character count.
//!
//! PART 9 §16 asks a continuation line for >= 2 columns, and §24 C1 gives a tab
//! a column value: it advances to the next multiple of 4 from wherever it
//! starts. So a bare tab reaches column 4 and continues the note exactly as two
//! literal spaces do (spec carve#796).
//!
//! This engine asked `leading_ws(line) >= body_indent` - a COUNT of whitespace
//! bytes - while dedenting with `strip_leading_columns`, so the two halves of
//! the same rule disagreed: a bare tab counted as one character and was
//! refused, and `<SPACE><TAB>` counted as two and was taken. carve-js had the
//! same split (carve-js#726) and carve-php the complementary one, two spaces or
//! a bare tab but never the mixture (carve-php#888).
//!
//! A refused continuation does not indent differently: it LEAVES the note and
//! becomes a top-level paragraph above the reference, so the content moves out
//! of the endnote and into the document body. Each case below asserts where the
//! text ended up.

use carve::{parse, render_html};

fn html(src: &str) -> String {
    render_html(&parse(src)).expect("render")
}

fn continues(indent: &str, blank: bool) -> bool {
    let src = format!(
        "[^a]: note\n{}{}more\n\nsee[^a]\n",
        if blank { "\n" } else { "" },
        indent
    );
    let out = html(&src);
    // Inside the note, rather than as a document-level paragraph.
    !out.contains("<p>more</p>") && out.contains("more")
}

#[test]
fn two_spaces_continue_the_note() {
    assert!(continues("  ", true), "two spaces reach column 2");
}

#[test]
fn a_bare_tab_continues_the_note() {
    assert!(continues("\t", true), "a tab reaches column 4");
}

#[test]
fn a_space_then_a_tab_continues_the_note() {
    assert!(
        continues(" \t", true),
        "a space then a tab also reaches column 4"
    );
}

#[test]
fn a_bare_tab_with_no_blank_line_before_it_continues_the_note() {
    assert!(continues("\t", false));
}

#[test]
fn one_space_is_still_not_a_continuation() {
    assert!(!continues(" ", true), "one space reaches only column 1");
}

#[test]
fn a_flush_left_line_is_still_not_a_continuation() {
    assert!(!continues("", true));
}

#[test]
fn the_dedent_is_by_column_not_by_character_count() {
    // The body's own column is 2. A tab reaching column 4 leaves two residual
    // columns, which the body's blocks read themselves - so the paragraph keeps
    // no leading tab and no code block appears.
    let out = html("[^a]: note\n\n\tmore\n\nsee[^a]\n");
    assert!(
        !out.contains("<pre>"),
        "residual columns became a code block: {out}"
    );
    assert!(out.contains("more"));
}
