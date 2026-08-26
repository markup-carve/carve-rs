//! BLOCK IMAGE IS A PROPERTY OF THE RESOLVED TREE (carve-rs#1444, spec
//! markup-carve/carve#1784 -- PART 9R R7, PART 12 section 23).
//!
//! `![a][r]` is a block image where `[r]: /u` is written and ordinary paragraph
//! text where it is not, and the definition may sit anywhere in the document --
//! so the question cannot be settled in the parser's forward pass.
//!
//! ONE promotion phase settles it after reference resolution, and it is the
//! only place that binds an image caption. Until it runs, a `^ ` line below an
//! image paragraph is an UNBOUND SLOT: not a caption, and not paragraph text.
//! The phase binds it where the paragraph is promoted, and hands its source
//! lines back -- ALL of them -- where it is not.
//!
//! The two give-back paths below are the ones on which a line of the document
//! can be lost: a slot MORE THAN ONE LINE wide, and a slot INSIDE A CONTAINER.
//! Corpus category 434 pins each with its resolved control beside it; before it
//! neither was held by any corpus document, and a mutation that gave back only
//! the first slot line failed just two tests on the oracle.

use carve::{parse, render_html};

fn html(src: &str) -> String {
    render_html(&parse(src)).trim().to_string()
}

#[test]
fn resolved_with_no_caption_is_a_bare_block_image() {
    assert_eq!(html("![a][r]\n\n[r]: /u\n"), "<img src=\"/u\" alt=\"a\">");
}

#[test]
fn resolved_with_a_caption_is_a_figure() {
    assert_eq!(
        html("![a][r]\n^ cap\n\n[r]: /u\n"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>cap</figcaption>\n</figure>"
    );
}

#[test]
fn unresolved_with_no_caption_is_an_ordinary_paragraph() {
    assert_eq!(html("![a][r]\n"), "<p>![a][r]</p>");
}

/// The row that decides the model. Binding the caption on the source shape
/// would put a `<figure>` around a paragraph of literal `![a][r]`, which no
/// engine writes.
#[test]
fn unresolved_with_a_caption_gives_the_slot_back_as_paragraph_text() {
    assert_eq!(html("![a][r]\n^ cap\n"), "<p>![a][r]\n^ cap</p>");
}

/// EVERY line of the slot, not the marker line alone. Handing back only the
/// first line loses `continued` from the document.
#[test]
fn gives_back_every_line_of_a_multi_line_slot() {
    assert_eq!(
        html("![a][r]\n^ cap one\ncontinued\n"),
        "<p>![a][r]\n^ cap one\ncontinued</p>"
    );
}

#[test]
fn binds_the_whole_multi_line_slot_when_the_reference_resolves() {
    assert_eq!(
        html("![a][r]\n^ cap one\ncontinued\n\n[r]: /u\n"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>cap one\ncontinued</figcaption>\n</figure>"
    );
}

#[test]
fn gives_the_slot_back_inside_a_list_item() {
    assert_eq!(
        html("- ![a][r]\n  ^ cap\n"),
        "<ul>\n  <li>![a][r]\n^ cap</li>\n</ul>"
    );
}

#[test]
fn binds_the_slot_inside_a_list_item_when_the_reference_resolves() {
    assert_eq!(
        html("- ![a][r]\n  ^ cap\n\n[r]: /u\n"),
        "<ul>\n  <li>\n    <figure>\n      <img src=\"/u\" alt=\"a\">\n      <figcaption>cap</figcaption>\n    </figure>\n  </li>\n</ul>"
    );
}

/// A control that isolates the container path: the inline form in the same
/// position.
#[test]
fn the_inline_form_in_the_same_position_keeps_its_caption() {
    assert_eq!(
        html("- ![a](/u)\n  ^ cap\n"),
        "<ul>\n  <li>\n    <figure>\n      <img src=\"/u\" alt=\"a\">\n      <figcaption>cap</figcaption>\n    </figure>\n  </li>\n</ul>"
    );
}

#[test]
fn binds_the_slot_inside_a_block_quote_when_the_reference_resolves() {
    assert_eq!(
        html("> ![a][r]\n> ^ cap\n\n[r]: /u\n"),
        "<blockquote>\n  <figure>\n    <img src=\"/u\" alt=\"a\">\n    <figcaption>cap</figcaption>\n  </figure>\n</blockquote>"
    );
}

#[test]
fn gives_the_slot_back_inside_a_block_quote() {
    assert_eq!(
        html("> ![a][r]\n> ^ cap\n"),
        "<blockquote><p>![a][r]\n^ cap</p></blockquote>"
    );
}
