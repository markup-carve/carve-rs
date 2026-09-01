//! A blank line between definition-list entries must not flip whether a
//! definition written as a description's body registers (#1500).
//!
//! Section 10 I5: a definition written at a definition body's content column is
//! an interrupter AND it registers. The parser already read the two spellings
//! the same way - a blank between entries does not end a definition list, it
//! only makes it loose, and both produce the same two descriptions. But the
//! gate that strips the `:  ` description marker asked the line DIRECTLY above,
//! so after a blank the marker went unstripped: the definition was consumed as
//! description text, collected by nobody, and the reference below it stayed
//! literal.
//!
//! carve-js and carve-php had the identical bug from the identical cause, fixed
//! in markup-carve/carve-js#1589 and markup-carve/carve-php#1845. Both gates in
//! this engine's two prepasses are the same expression, so the footnote body
//! carried it too.
//!
//! The blank is transparent only WHILE the list is open. Prose or a heading
//! between the entries really does end it, and the previous non-blank line is
//! then that block rather than a description - which is what keeps the refusals
//! below refusing.

use carve::to_html;

#[test]
fn a_description_body_definition_registers_across_a_blank() {
    let html = to_html(":: term\n:  def\n\n:  [r]: /url\n\n[r][]\n");

    assert!(html.contains("href=\"/url\""), "{html}");
    assert!(!html.contains("[r]: /url"), "{html}");
}

#[test]
fn it_registers_when_the_description_is_the_only_one() {
    let html = to_html(":: term\n\n:  [r]: /url\n\n[r][]\n");

    assert!(html.contains("href=\"/url\""), "{html}");
}

#[test]
fn the_description_is_emptied_the_way_the_adjacent_spelling_empties_it() {
    // The two spellings must agree on BOTH halves. Asserting only resolution
    // would pass on an engine that resolved the reference and still left the
    // author's line sitting in the `dd`.
    let across = to_html(":: term\n:  def\n\n:  [r]: /url\n\n[r][]\n");
    let adjacent = to_html(":: term\n:  def\n:  [r]: /url\n\n[r][]\n");

    assert!(across.contains("<dd></dd>"), "{across}");
    assert!(adjacent.contains("<dd></dd>"), "{adjacent}");
}

#[test]
fn the_adjacent_spelling_is_unchanged() {
    // The control: the shape that always worked must keep working.
    assert!(to_html(":: term\n:  def\n:  [r]: /url\n\n[r][]\n").contains("href=\"/url\""));
}

#[test]
fn a_footnote_body_definition_registers_across_a_blank_too() {
    // The footnote prepass carries the same gate, so the same blank hid the
    // same definition: the note went unregistered and `see [^a]` stayed
    // literal.
    let html = to_html(":: term\n:  def\n\n:  [^a]: note\n\nsee [^a]\n");

    assert!(html.contains("role=\"doc-endnotes\""), "{html}");
    assert!(!html.contains("[^a]: note"), "{html}");
}

#[test]
fn prose_between_the_entries_ends_the_list_and_refuses() {
    let html = to_html(":: term\n:  def\n\npara\n\n:  [r]: /url\n\n[r][]\n");

    assert!(!html.contains("href=\"/url\""), "{html}");
    assert!(html.contains(":  [r]: /url"), "{html}");
}

#[test]
fn a_heading_between_the_entries_ends_it_too() {
    let html = to_html(":: term\n:  def\n\n# h\n\n:  [r]: /url\n\n[r][]\n");

    assert!(!html.contains("href=\"/url\""), "{html}");
}

#[test]
fn a_description_marker_with_no_list_above_registers_nothing() {
    // Corpus 216: a lone `:  ` line is not a description, so what follows its
    // marker is not a definition. The carry must not invent a term that no
    // line above ever wrote.
    let html = to_html("para\n\n:  [r]: /url\n\n[r][]\n");

    assert!(!html.contains("href=\"/url\""), "{html}");
}
