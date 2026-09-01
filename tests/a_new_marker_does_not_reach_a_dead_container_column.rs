//! PART 0, A NEW MARKER DOES NOT REACH A DEAD CONTAINER'S COLUMN (carve#1892).
//!
//! A bare blank ends every open quote and every column opened inside one dies
//! with it, so a later line that writes the marker again opens a NEW quote and
//! inherits nothing. A definition two columns above that quote's content column
//! with no item open is paragraph text: published where it was written and
//! registering nothing.
//!
//! The parser already rendered that document as two separate quotes, but the
//! column frames outlived the quote that held them, so the definition was
//! consumed and registered and its quote came back empty.

use carve::to_html;

#[test]
fn the_definition_is_published_and_registers_nothing() {
    let html = to_html("> - x\n\n>   [r]: /u\n\nsee [t][r]\n");

    assert!(html.contains("[r]: /u"), "line not published: {html}");
    assert!(!html.contains("href=\"/u\""), "still registered: {html}");
}

#[test]
fn it_holds_for_an_ordered_item_too() {
    let html = to_html("> 1. x\n\n>    [r]: /u\n\nsee [t][r]\n");

    assert!(!html.contains("href=\"/u\""), "still registered: {html}");
}

/// A quote-marked blank closes nothing - it is the quote's own continuation -
/// so the item column survives it and the definition still registers.
#[test]
fn a_quote_marked_blank_still_registers() {
    let html = to_html("> - x\n>\n>   [r]: /u\n>\n> see [t][r]\n");

    assert!(html.contains("href=\"/u\""), "not resolved: {html}");
}

#[test]
fn the_adjacent_shape_still_registers() {
    let html = to_html("> - x\n>   [r]: /u\n>\n> see [t][r]\n");

    assert!(html.contains("href=\"/u\""), "not resolved: {html}");
}

/// At document level a list item IS transparent across a blank, so level 0
/// survives the truncation and the unquoted shape is untouched.
#[test]
fn an_unquoted_loose_item_still_registers() {
    let html = to_html("- x\n\n  [r]: /u\n\nsee [t][r]\n");

    assert!(html.contains("href=\"/u\""), "not resolved: {html}");
}
