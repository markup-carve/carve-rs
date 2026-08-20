//! A footnote body whose last block is not a paragraph gets a SYNTHESIZED
//! paragraph to carry the backlink (carve-rs#667, spec markup-carve/carve#799).
//!
//! The backlink used to be placed by searching the rendered string for its last
//! `</p>`, which was wrong in two different ways depending on what the body
//! ended with:
//!
//! - a body ending in a fenced code block has no `</p>` at all, so the anchor
//!   was appended bare after `</pre>` - leaving the endnote ending in something
//!   that is not a block-level element;
//! - a body ending in a quote or a list DOES contain a `</p>`, nested inside
//!   that block, so the backlink was placed inside the quotation itself.
//!
//! Only a last block that IS a paragraph takes the backlink inside it. That is
//! the common case and was always right.

fn note_body(source: &str) -> String {
    let html = carve::to_html(source);
    let start = html.find("<li id=\"fn1\">").expect("an endnote");
    // The endnote's OWN closing tag, which is the one at the list's indent. A
    // plain search for `</li>` finds the first nested item's instead, and then a
    // body ending in a list looks empty.
    let end = html[start..].find("\n    </li>").expect("its end") + start;
    html[start..end].to_string()
}

const BACKLINK: &str =
    "<p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>";

#[test]
fn a_body_ending_in_a_fence_gets_its_own_paragraph() {
    let body = note_body("[^a]: note\n\n  ```\n  code\n  ```\n\nsee[^a]\n");
    assert!(
        body.contains(BACKLINK),
        "expected a synthesized paragraph, got:\n{body}"
    );
    assert!(
        !body.contains("</code></pre><a"),
        "backlink is still glued to the fence:\n{body}"
    );
}

#[test]
fn a_body_ending_in_a_quote_does_not_take_the_backlink_inside_the_quote() {
    let body = note_body("[^a]: note\n\n  > quoted\n\nsee[^a]\n");
    assert!(
        body.contains(BACKLINK),
        "expected a synthesized paragraph, got:\n{body}"
    );
    // The nested paragraph is the quotation's own text, and the backlink is not
    // part of what was quoted.
    assert!(
        !body.contains("quoted<a href=\"#fnref1\""),
        "backlink landed inside the quotation:\n{body}"
    );
}

#[test]
fn a_body_ending_in_a_list_does_not_take_the_backlink_inside_the_item() {
    let body = note_body("[^a]: note\n\n  - item\n\nsee[^a]\n");
    assert!(
        body.contains(BACKLINK),
        "expected a synthesized paragraph, got:\n{body}"
    );
    assert!(
        !body.contains("item<a href=\"#fnref1\""),
        "backlink landed inside the list item:\n{body}"
    );
}

#[test]
fn a_body_ending_in_a_table_gets_its_own_paragraph() {
    let body = note_body("[^a]: note\n\n  | a |\n  |---|\n  | b |\n\nsee[^a]\n");
    assert!(
        body.contains(BACKLINK),
        "expected a synthesized paragraph, got:\n{body}"
    );
}

#[test]
fn a_body_ending_in_a_paragraph_still_takes_the_backlink_inside_it() {
    // The case that must NOT change: no synthesized paragraph, the backlink
    // sits at the end of the body's own last paragraph.
    let body = note_body("[^a]: note\n\n  more\n\nsee[^a]\n");
    assert!(
        body.contains("more<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>"),
        "expected the backlink inside the last paragraph, got:\n{body}"
    );
    assert!(
        !body.contains(BACKLINK),
        "a paragraph was synthesized where the body already ended in one:\n{body}"
    );
}

#[test]
fn a_one_line_note_still_takes_the_backlink_inside_its_paragraph() {
    let body = note_body("[^a]: note\n\nsee[^a]\n");
    assert!(
        body.contains("note<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>"),
        "expected the backlink inside the note's paragraph, got:\n{body}"
    );
}

#[test]
fn a_synthesized_paragraph_carries_every_backlink_of_a_repeated_note() {
    // A note referenced more than once gets one numbered backlink per
    // reference; they all belong in the one synthesized paragraph.
    let body = note_body("[^a]: note\n\n  ```\n  code\n  ```\n\nsee[^a] and[^a]\n");
    assert!(
        body.contains("↩<sup>1</sup>") && body.contains("↩<sup>2</sup>"),
        "expected both numbered backlinks, got:\n{body}"
    );
    assert_eq!(
        body.matches("<p><a href=\"#fnref").count(),
        1,
        "expected exactly one synthesized paragraph, got:\n{body}"
    );
}
