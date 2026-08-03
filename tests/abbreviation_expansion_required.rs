//! PART 5: `abbreviation_expansion = {character - newline}+` - ONE or more.
//!
//! An empty expansion is not a definition, so the line stays paragraph text.
//! This engine consumed it, which deleted it from the document; carve-js
//! implements the production literally and keeps it.
//!
//! It is the last definition kind where that was still true. A link reference
//! and a footnote definition with no content are already kept as text in all
//! three engines - the footnote case since carve-rs#482 - so the abbreviation
//! was the odd one out, in this engine and carve-php.
//!
//! Note the boundary the grammar draws, which carve-js already follows: a
//! SECOND trailing space IS an expansion, because a space is a character.

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn an_empty_expansion_is_not_a_definition() {
    assert_eq!(squash(&to_html("*[A]: \n")), "<p>*[A]:</p>");
}

#[test]
fn nothing_is_silently_dropped() {
    // The sharp end: the line rendered NOTHING, so it vanished.
    assert!(!to_html("*[A]: \n").trim().is_empty());
}

#[test]
fn one_character_of_expansion_is_enough() {
    // A second space is a character, so this IS a definition - unused, so it
    // renders nothing. Pinned because it is the boundary, not an accident.
    assert_eq!(to_html("*[A]:  \n").trim(), "");
    assert_eq!(to_html("*[A]: \t\n").trim(), "");
}

#[test]
fn a_real_definition_still_works() {
    let html = to_html("*[HTML]: HyperText Markup Language\n\nHTML rules.\n");

    assert!(html.contains("<abbr"), "{html}");
    assert!(html.contains("HyperText Markup Language"), "{html}");
}

#[test]
fn no_separator_space_is_unchanged() {
    // Already correct in every engine.
    assert_eq!(squash(&to_html("*[A]:\n")), "<p>*[A]:</p>");
}
