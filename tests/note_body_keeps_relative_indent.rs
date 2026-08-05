//! A footnote body keeps the relative indentation the author wrote.
//!
//! `extract_footnote_defs` collected each body line with `trim_ascii_start`,
//! which removes ALL leading whitespace rather than the body's own indent. Every
//! line then arrived at the block parser flush left, so a nested list marker sat
//! at the same column as its parent and the sublist flattened into siblings
//! (#611). carve-js and carve-php both nest it, and this engine nests the same
//! two lines in a block quote, a div and a list item - the note body was the one
//! container that lost the structure.
//!
//! Indentation inside a note body is not decoration: it is what says which item
//! a marker belongs to, how far a continuation line reaches, and where a fenced
//! block's content starts.

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn a_nested_list_in_a_note_body_stays_nested() {
    let html = squash(&to_html("[^a]: note\n\n  - one\n    - deep\n\nsee[^a]"));

    assert!(
        html.contains("<li>one <ul> <li>deep</li> </ul> </li>"),
        "the sublist was flattened: {html}"
    );
}

#[test]
fn the_flattened_form_is_gone() {
    let html = squash(&to_html("[^a]: note\n\n  - one\n    - deep\n\nsee[^a]"));

    assert!(
        !html.contains("<li>one</li> <li>deep</li>"),
        "`deep` is still a sibling of `one`: {html}"
    );
}

#[test]
fn a_three_level_list_keeps_both_levels() {
    let html = squash(&to_html(
        "[^a]: note\n\n  - one\n    - two\n      - three\n\nsee[^a]",
    ));

    assert!(html.contains("<li>two <ul> <li>three</li>"), "{html}");
}

#[test]
fn a_continuation_line_still_folds_into_its_item() {
    // The other thing the indent decides. A line indented to the item's content
    // column continues that item rather than starting a block.
    let html = squash(&to_html("[^a]: note\n\n  - one\n    more\n\nsee[^a]"));

    assert!(html.contains("<li>one more</li>"), "{html}");
}

#[test]
fn a_code_block_in_a_note_body_keeps_its_interior_indentation() {
    // A trim would also eat the indentation INSIDE verbatim content, where it is
    // the author's text rather than layout.
    let html = to_html("[^a]: note\n\n  ```\n  outer\n    inner\n  ```\n\nsee[^a]");

    assert!(
        html.contains("outer\n  inner"),
        "code interior lost its indent: {html}"
    );
}

#[test]
fn a_flush_left_body_is_unchanged() {
    // The boundary: a body written at the minimum indent has nothing to strip,
    // and must not gain any.
    let html = squash(&to_html("[^a]: note\n  more\n\nsee[^a]"));

    assert!(html.contains("<p>note more"), "{html}");
}
