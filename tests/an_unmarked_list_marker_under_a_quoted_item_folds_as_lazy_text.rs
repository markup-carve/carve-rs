//! An unmarked list marker below a quoted item ends the quote and opens a list
//! after it (markup-carve/carve#4860). It never opens a second item inside the
//! quote: the marker carries no `>`. A plain unmarked line and a quoted paragraph
//! still fold the line as lazy text (markup-carve/carve#1904, #1905).

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

/// THE REPORTED DOCUMENT. `- m` under a quoted item ends the quote and opens a
/// list after it; it does not open a second item inside the quote.
#[test]
fn the_reported_document_folds_the_marker_as_text() {
    assert_eq!(
        flat("> - a\n- m\ntail\n"),
        "<blockquote> <ul> <li>a</li> </ul> </blockquote> <ul> <li>m tail</li> </ul>"
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

/// A DEEPER quote answers the same way: the marker misses the item body and
/// both quotes, so it opens a list after them (carve#4860).
#[test]
fn a_deeper_quote_answers_the_same_way() {
    assert_eq!(
        flat("> > - a\n- m\ntail\n"),
        "<blockquote> <blockquote> <ul> <li>a</li> </ul> </blockquote> </blockquote> <ul> <li>m tail</li> </ul>"
    );
}

/// OTHER MARKER KINDS answer identically - ordered and star end the quote too.
#[test]
fn other_marker_kinds_fold_identically() {
    assert_eq!(
        flat("> - a\n1. m\ntail\n"),
        "<blockquote> <ul> <li>a</li> </ul> </blockquote> <ol> <li>m tail</li> </ol>"
    );
    assert_eq!(
        flat("> - a\n* m\ntail\n"),
        "<blockquote> <ul> <li>a</li> </ul> </blockquote> <ul> <li>m tail</li> </ul>"
    );
}

/// AN INDENTED unmarked marker ends the quote too: a line without `>` has no
/// column in the quote, so its indentation cannot open a sublist there.
#[test]
fn an_indented_unmarked_marker_still_folds() {
    assert_eq!(
        flat("> - a\n    - m\ntail\n"),
        "<blockquote> <ul> <li>a</li> </ul> </blockquote> <ul> <li>m tail</li> </ul>"
    );
}
