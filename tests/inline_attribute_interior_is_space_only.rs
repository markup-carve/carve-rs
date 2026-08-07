//! THE INLINE INTERIOR IS SPACE-ONLY, THE BLOCK-ATTRIBUTE LINE IS NOT
//! (PART 4, carve#906).
//!
//! Every whitespace slot of the INLINE attribute block takes `space`. FIVE
//! POSITIONS, and they revert in five separate places, so each is asserted one
//! at a time:
//!
//!   - the run after `{`
//!   - the run between two attributes
//!   - the run before `}`
//!   - the boundary after an UNQUOTED value
//!   - the blessed empty block `{ }`
//!
//! All five sit AFTER the first non-whitespace character of their line, which
//! is where PART 7's rule already says a tab is not syntax. A tab at any of
//! them makes the block unrecognized, and its braces show.
//!
//! THE EMPTY BLOCK IS A SEPARATE POSITION rather than a use of the separator,
//! and it is the one most likely to be missed: the executable spec needed two
//! edits for exactly this reason - narrowing the separator alone left
//! `[x]{<TAB>}` a valid empty block, and the corpus document that pins it
//! stayed green.
//!
//! THE BLOCK-ATTRIBUTE LINE KEEPS `whitespace` at all three of its slots, and
//! that distinction is the ruling rather than an omission. It is the one
//! construct whose interior can hold a leading indentation run: after a
//! `continuation`, the next line's leading whitespace IS indentation, and the
//! rule that narrows the inline block is the same rule that protects this one.
//! A fix that narrowed both surfaces at once fails the two controls at the
//! bottom of this file.

use carve::to_html;

// ---------------------------------------------------------------------------
// The five inline positions
// ---------------------------------------------------------------------------

#[test]
fn a_tab_does_not_separate_two_attributes() {
    assert_eq!(
        to_html("*x*{.a\t.b}\n").trim(),
        "<p><strong>x</strong>{.a\t.b}</p>"
    );
}

#[test]
fn a_tab_does_not_pad_after_the_opening_brace() {
    assert_eq!(
        to_html("*y*{\t.c}\n").trim(),
        "<p><strong>y</strong>{\t.c}</p>"
    );
}

#[test]
fn a_tab_does_not_pad_before_the_closing_brace() {
    assert_eq!(
        to_html("*z*{.d\t}\n").trim(),
        "<p><strong>z</strong>{.d\t}</p>"
    );
}

#[test]
fn a_tab_after_an_unquoted_value_ends_the_value_and_satisfies_no_separator() {
    // So the whole block fails, rather than the value merely stopping.
    assert_eq!(
        to_html("*x*{k=a\t.b}\n").trim(),
        "<p><strong>x</strong>{k=a\t.b}</p>"
    );
}

#[test]
fn a_tab_in_the_blessed_empty_block_is_not_padding() {
    assert_eq!(to_html("[x]{\t}\n").trim(), "<p>[x]{\t}</p>");
}

// ---------------------------------------------------------------------------
// It is the WHITESPACE that is narrowed, not the tab alone
// ---------------------------------------------------------------------------

#[test]
fn a_no_break_space_does_not_separate_two_attributes_either() {
    // The tokenizer split on the Unicode whitespace property, so this formed
    // two attributes. A no-break space is content rather than syntax, and the
    // executable spec renders the literal - as it does for an ideographic
    // space.
    assert_eq!(to_html("[x]{a\u{a0}b}\n").trim(), "<p>[x]{a&nbsp;b}</p>");
    assert_eq!(
        to_html("[x]{a\u{3000}b}\n").trim(),
        "<p>[x]{a\u{3000}b}</p>"
    );
}

// ---------------------------------------------------------------------------
// CONTROLS
// ---------------------------------------------------------------------------

#[test]
fn control_a_tab_inside_a_quoted_value_is_content() {
    assert_eq!(
        to_html("*y*{k=\"a\tb\"}\n").trim(),
        "<p><strong k=\"a\tb\">y</strong></p>"
    );
}

#[test]
fn control_the_space_spelling_of_every_position_still_works() {
    assert_eq!(
        to_html("*x*{ .a  .b }\n").trim(),
        "<p><strong class=\"a b\">x</strong></p>"
    );
    assert_eq!(to_html("[x]{ }\n").trim(), "<p><span>x</span></p>");
    assert_eq!(
        to_html("*x*{k=a .b}\n").trim(),
        "<p><strong k=\"a\" class=\"b\">x</strong></p>"
    );
}

#[test]
fn control_the_block_attribute_line_keeps_whitespace() {
    // Both of these stay valid, and they are the ruling rather than an
    // omission.
    assert_eq!(
        to_html("{\t.a\t.b\t}\n\nparagraph\n").trim(),
        "<p class=\"a b\">paragraph</p>"
    );
    assert_eq!(
        to_html("{.a\n\t.b}\n\nparagraph\n").trim(),
        "<p class=\"a b\">paragraph</p>"
    );
}

#[test]
fn control_a_reference_definitions_trailing_block_reads_the_same_production() {
    // `reference_definition` is spelled `[space, attributes]`, so it narrows
    // with the inline block rather than separately - one production, one
    // answer. (The line is anchored on top of that, so a tab in the interior
    // takes the whole line to a paragraph rather than only the block.)
    assert!(!to_html("[a]: /u {.a\t.b}\n\n[a][]\n").contains("class="));
    assert!(to_html("[a]: /u {.a .b}\n\n[a][]\n").contains("class=\"a b\""));
}
