//! The authored block base belongs to the INNERMOST open container.
//!
//! PART 9 §24 C3, ruled in markup-carve/carve#1781 and extended to a list item
//! in markup-carve/carve#1791: at or above a container's minimum content
//! column, a recognized block opener establishes a local block base, and that
//! question is asked of the innermost container open where the line is written,
//! never of an outer one. One rule, one statement, and the same one for a list
//! item, a footnote body, a definition description and a `+`-attached block. It
//! replaces three per-container spellings that disagreed.
//!
//! This engine registered a nested LIST MARKER's content column and nothing
//! else (markup-carve/carve-rs#1430). A definition description opened inside a
//! footnote body or a list item was not a container to the pass that decides
//! ownership, so two things went wrong at once:
//!
//! A block written AT the description's own content column was measured against
//! the OUTER body, rebased to its column and lifted out of the description it
//! was written into.
//!
//! A block written BELOW that column was carried along by the description's
//! rebased run and dedented by the run's base alone. That lands it between the
//! two columns - too shallow to be the description's content, no longer at the
//! body's minimum - where the strict column-0 rule reads it as literal text. So
//! the one band in the ladder where the description ENDS was also the one band
//! where a quote stopped being a quote.
//!
//! THE LADDER HAS ONE BOUNDARY NOW. Below the description's content column the
//! description ends and the block is the surviving container's; at or above it
//! the block is the description's. The same boundary, in the same place,
//! whether the outer container is a footnote body or a list item and whether or
//! not a blank line separates the block - swept in
//! `a_blank_line_does_not_end_a_definition_s_authored_base.rs`, which used to
//! pin the superseded reading and now pins this one.
//!
//! These documents are spec corpus categories 419, 422 and 423 at carve
//! `6dac47e2`. This repo's spec pin is `70e46b55`, whose corpus tops out at
//! category 418, so the corpus gate cannot exercise them and they are written
//! out here instead.

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

// ---------------------------------------------------------------------------
// At the description's column, the block is the description's
// ---------------------------------------------------------------------------

#[test]
fn a_quote_at_a_nested_descriptions_column_is_the_descriptions() {
    // The footnote body's minimum column is 2 and the description opens column 5,
    // which is where the quote is written. The innermost container open there is
    // the description, so the quote is its block - not the note's.
    assert_eq!(
        html("[^n]: intro\n\n  :: term\n  :  definition\n\n     > quote\n\nsee[^n]\n"),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>intro</p>\n      <dl>\n        <dt>term</dt>\n        <dd>\n          <p>definition</p>\n          <blockquote><p>quote</p></blockquote>\n        </dd>\n      </dl>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn the_same_document_answers_the_same_way_inside_a_list_item() {
    // One container down, and the rule does not change. carve#1791 is the half of
    // the ruling that says so: a list item is a container the rule reaches, and
    // the outer container's kind never enters the question.
    assert_eq!(
        html("- intro\n\n   :: term\n   :  definition\n\n      > quote\n"),
        "<ul>\n  <li>intro\n    <dl>\n      <dt>term</dt>\n      <dd>\n        <p>definition</p>\n        <blockquote><p>quote</p></blockquote>\n      </dd>\n    </dl>\n  </li>\n</ul>"
    );
}

// ---------------------------------------------------------------------------
// Below it, the description ends and the block is still a block
// ---------------------------------------------------------------------------

#[test]
fn below_the_descriptions_column_the_quote_is_the_bodys_own() {
    // The description opens column 6 and the quote is written at 5, so the
    // description ends and the surviving container is the footnote body. A quote
    // is still a quote there. It used to come back as a paragraph holding a
    // literal `> quote`.
    assert_eq!(
        html("[^n]: intro\n\n   :: term\n   :  definition\n\n     > quote\n\nsee[^n]\n"),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>intro</p>\n      <dl>\n        <dt>term</dt>\n        <dd>definition</dd>\n      </dl>\n      <blockquote><p>quote</p></blockquote>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn the_blank_line_above_it_makes_no_difference() {
    // The same band written tight. A blank line loosens a description; it does
    // not decide which container owns the line below it.
    assert_eq!(
        html("[^n]: intro\n\n   :: term\n   :  definition\n    > quote\n\nsee[^n]\n"),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>intro</p>\n      <dl>\n        <dt>term</dt>\n        <dd>definition</dd>\n      </dl>\n      <blockquote><p>quote</p></blockquote>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

// ---------------------------------------------------------------------------
// The list marker the pass already registered, unmoved
// ---------------------------------------------------------------------------

#[test]
fn a_quote_at_a_nested_items_content_column_stays_in_the_item() {
    // carve#1791's own document, and the case this engine already read correctly:
    // a nested list marker registers its content column, so the quote belongs to
    // the item it was written into. Adding the description registration beside it
    // does not disturb it.
    assert_eq!(
        html("[^n]: intro\n\n  - item\n\n    > quote\n\nsee[^n]\n"),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>intro</p>\n      <ul>\n        <li>item\n          <blockquote><p>quote</p></blockquote>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

// ---------------------------------------------------------------------------
// Only an OPENER leaves the description's run
// ---------------------------------------------------------------------------

#[test]
fn prose_in_the_same_band_is_not_an_opener() {
    // The bound is "a recognized block opener", and this is the half of it that
    // no assertion reached. The description opens column 5 and the text is
    // written at 4, which is exactly the band the openers above move in - past
    // the run's base, short of the description's content column. Prose written
    // there is not the body's own block: the description ends and the line is
    // classified in the surviving context, as a paragraph of the note.
    //
    // Dropping the opener test from the bound is otherwise nearly invisible.
    // Measured on this tree over 1207 generated definition-in-body shapes, it
    // moves 14, and every one is prose separated from the description by a
    // blank line - so the term was pinned by nothing at all. Without it the
    // line stays in the run and comes back inside the `<dd>` as a second
    // paragraph.
    assert_eq!(html("[^n]: intro\n\n   :: term\n   : definition\n\n    text\n\nsee[^n]\n"), "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>intro</p>\n      <dl>\n        <dt>term</dt>\n        <dd>definition</dd>\n      </dl>\n      <p>text<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>");
}

#[test]
fn prose_in_a_list_item_is_not_an_opener_either() {
    // The same claim with the outer container swapped, because the bound runs
    // in the list-item call as well.
    assert_eq!(html("- item\n\n   :: term\n   : definition\n\n    text\n"), "<ul>\n  <li><p>item</p>\n    <dl>\n      <dt>term</dt>\n      <dd>definition</dd>\n    </dl>\n    <p>text</p>\n  </li>\n</ul>");
}
