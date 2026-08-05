//! A definition on a footnote body's continuation line is collected.
//!
//! The note body is lifted out of the document before the link-definition pass
//! runs, so a `[r]: /u` written inside one was never offered to that pass. It
//! stayed in the body and rendered as text, and the reference below it never
//! resolved (#599). carve-js, carve-php and the executable spec all collect it -
//! a definition inside a container is document-level metadata, which §16
//! already says for a list item and a block quote.
//!
//! Indent 2 is the note body's own continuation column, so these are genuine
//! body lines. What happens at indents 1 and 3 - below and past the column - is
//! the open question in markup-carve/carve#664 and #669, where the engines give
//! three answers; nothing here depends on how that is settled.

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn a_definition_in_a_note_body_resolves_a_reference_below_it() {
    let html = squash(&to_html("[^a]: note\n  [r]: /u\n\nsee[^a] and [t][r]"));

    assert!(
        html.contains("href=\"/u\""),
        "the definition did not register: {html}"
    );
    assert!(
        !html.contains("[t][r]"),
        "the reference stayed literal: {html}"
    );
}

#[test]
fn the_definition_line_renders_nothing_inside_the_note() {
    let html = squash(&to_html("[^a]: note\n  [r]: /u\n\nsee[^a] and [t][r]"));

    assert!(
        !html.contains("[r]: /u"),
        "the definition rendered as note text: {html}"
    );
    assert!(
        html.contains("note"),
        "the note body lost its own text: {html}"
    );
}

#[test]
fn a_footnote_definition_in_a_note_body_still_works_the_old_way() {
    // The boundary: this pass only claims LINK definitions. A nested footnote
    // definition is not one, and its handling must not move.
    let html = squash(&to_html("[^a]: note\n\n[^b]: other\n\nsee[^a] and[^b]"));

    assert!(html.contains("note"), "{html}");
    assert!(html.contains("other"), "{html}");
}

#[test]
fn a_top_level_definition_wins_over_one_in_a_note_body() {
    // Measured on carve-js and carve-php: with both present, `[t][r]` resolves
    // to the top-level target, whichever comes first in the source.
    let html = squash(&to_html(
        "[^a]: note\n  [r]: /inner\n\n[r]: /outer\n\nsee[^a] and [t][r]",
    ));

    assert!(html.contains("href=\"/outer\""), "{html}");
    assert!(!html.contains("/inner"), "{html}");
}

#[test]
fn a_definition_inside_a_code_fence_in_the_body_is_content() {
    // A code fence is opaque, and every engine agrees about that at the TOP
    // level - none registers `[r]: /u` written inside ```. Collecting it here
    // consumed the line, so the code block came out empty and the reference
    // resolved from something the author had quoted rather than defined.
    let html = squash(&to_html(
        "[^a]: note\n  ```\n  [r]: /u\n  ```\n\nsee[^a] and [t][r]",
    ));

    assert!(
        html.contains("[r]: /u"),
        "the code line was swallowed: {html}"
    );
    assert!(
        html.contains("[t][r]"),
        "a quoted definition resolved a reference: {html}"
    );
}

#[test]
fn the_last_definition_among_note_bodies_wins() {
    // `extract_link_defs` lets a later document definition overwrite an earlier
    // one; note bodies follow the same rule so placement does not change the
    // precedence a label gets.
    let html = squash(&to_html(
        "[^a]: one\n  [r]: /first\n\n[^b]: two\n  [r]: /second\n\nsee[^a][^b] and [t][r]",
    ));

    assert!(html.contains("href=\"/second\""), "{html}");
}

#[test]
fn a_definition_shaped_line_with_no_target_stays_text() {
    // `[r]:` with nothing after it is not a definition anywhere else either, so
    // it must stay visible rather than being consumed into the table.
    let html = squash(&to_html("[^a]: note\n  [r]:\n\nsee[^a]"));

    assert!(
        html.contains("[r]:"),
        "a non-definition line was swallowed: {html}"
    );
}
