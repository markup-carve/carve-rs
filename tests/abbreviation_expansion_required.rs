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
//! The boundary MOVED with carve#892, which spells the marker-to-content
//! separator `space+`. A second trailing space used to be an expansion, because
//! a space is a character; it is now part of the separator RUN, so a line with
//! only spaces after the marker has no expansion and stays paragraph text. The
//! executable spec answers `<p>*[A]:</p>` for it.

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
fn a_spaces_only_line_has_no_expansion() {
    // MARKER REQUIRES CONTENT after the run. Under carve#892 the second space
    // is separator rather than expansion, so this is a paragraph and not a
    // definition with an empty title. Pinned because it is the boundary, not an
    // accident, and measured against the executable spec.
    assert_eq!(squash(&to_html("*[A]:  \n")), "<p>*[A]:</p>");
    assert_eq!(squash(&to_html("*[A]:     \n")), "<p>*[A]:</p>");
}

#[test]
fn one_character_of_expansion_is_enough() {
    // A TAB after the run is a character of the expansion, so this IS a
    // definition - unused, so it renders nothing. (The oracle additionally
    // drops the trailing tab under carve#926 and reads the whole line as a
    // paragraph; that rule is carve-rs#751 and moves this line when it lands.)
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
