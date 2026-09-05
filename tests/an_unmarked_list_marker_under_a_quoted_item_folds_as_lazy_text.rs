//! An unmarked list marker below a quoted item folds as that item's lazy text,
//! not a second list item (markup-carve/carve#1904).
//!
//! `> - a` / `- m`: the `- m` carries no `>`, so it is not the quote's content
//! at any column (PART 0). It continues the item's open paragraph as lazy text,
//! exactly as the paragraph-trailing spelling `> a` / `- m` does (#1905) - #1904
//! is the LIST-trailing variant of the same rule. Opening a second `<li>` inside
//! the quote is a third answer no reading permits: the marker never carried the
//! `>` that would put it inside the quote.
//!
//! This is the lazy-fold machinery landed for the nested-quote def-list
//! reconciliation (markup-carve/carve-rs#1537): an unmarked block-opener line
//! folding into an open quoted item is lazy text, and the fix generalized the
//! rule from the paragraph case to the list-trailing case. Before #1537 this
//! engine opened the wrong second item; these rows pin that it does not return.
//!
//! ORACLE: the executable spec (`scripts/spec/layout.mjs` + `html.mjs`) at spec
//! main `586d4072`, which produces the lazy fold on every row here; a 168-doc
//! sweep of the family (four container prefixes x six marker kinds x seven
//! columns) agrees with this engine on every document.

use carve::{to_html, to_html_with_options, Options};

/// The library facade and the position-tracking path must agree - the #908
/// guard. Every row runs through both.
fn flat(source: &str) -> String {
    let facade = to_html(source);
    let positions = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(
        facade, positions,
        "the library path and the position-tracking path disagree on {source:?}"
    );
    facade.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// THE REPORTED DOCUMENT. `- m` under a quoted item folds as the item's lazy
/// text; it does not open a second item.
#[test]
fn the_reported_document_folds_the_marker_as_text() {
    assert_eq!(
        flat("> - a\n- m\ntail\n"),
        "<blockquote> <ul> <li>a - m tail</li> </ul> </blockquote>"
    );
}

/// THE #1905 REFERENCE, the paragraph-trailing spelling this generalizes: a
/// quoted paragraph folds the same `- m` as text.
#[test]
fn the_paragraph_trailing_spelling_answers_the_same_way() {
    assert_eq!(
        flat("> a\n- m\ntail\n"),
        "<blockquote><p>a - m tail</p></blockquote>"
    );
}

/// THE CONTROL THAT OPENS A SECOND ITEM: the marker CARRIES its own `>`, so it
/// is the quote's content and does open the second `<li>`.
#[test]
fn a_marker_carrying_its_own_quote_opens_the_second_item() {
    assert_eq!(
        flat("> - a\n> - m\ntail\n"),
        "<blockquote> <ul> <li>a</li> <li>m tail</li> </ul> </blockquote>"
    );
}

/// A PLAIN unmarked line folds too - the list marker is not what makes `- m`
/// lazy text, the missing `>` is.
#[test]
fn a_plain_unmarked_line_folds_as_well() {
    assert_eq!(
        flat("> - a\nm\ntail\n"),
        "<blockquote> <ul> <li>a m tail</li> </ul> </blockquote>"
    );
}

/// A DEEPER quote answers the same way: the marker still carries no `>`.
#[test]
fn a_deeper_quote_answers_the_same_way() {
    assert_eq!(
        flat("> > - a\n- m\ntail\n"),
        "<blockquote> <blockquote> <ul> <li>a - m tail</li> </ul> </blockquote> </blockquote>"
    );
}

/// OTHER MARKER KINDS fold identically - ordered and star are lazy text too.
#[test]
fn other_marker_kinds_fold_identically() {
    assert_eq!(
        flat("> - a\n1. m\ntail\n"),
        "<blockquote> <ul> <li>a 1. m tail</li> </ul> </blockquote>"
    );
    assert_eq!(
        flat("> - a\n* m\ntail\n"),
        "<blockquote> <ul> <li>a * m tail</li> </ul> </blockquote>"
    );
}

/// AN INDENTED unmarked marker folds too: a lazy line has no column in the
/// quote, so its indentation cannot open a sublist either.
#[test]
fn an_indented_unmarked_marker_still_folds() {
    assert_eq!(
        flat("> - a\n    - m\ntail\n"),
        "<blockquote> <ul> <li>a - m tail</li> </ul> </blockquote>"
    );
}
