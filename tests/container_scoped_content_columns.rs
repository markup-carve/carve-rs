//! A list item's content column is live only inside the container it was
//! measured in.
//!
//! The definition prepasses track content columns so a definition on an item's
//! CONTINUATION line still reads as one. Measuring those columns inside a block
//! quote (#587) left the tracker unable to tell one container from another: an
//! indented line inside a quote matched a column belonging to a list that had
//! already closed, and a definition registered from a line that was not in that
//! list at all (#593).
//!
//! Columns are scoped per container now - one frame per open quote level - so
//! whether a column is live is answered by structure, not by comparing indents
//! measured in whichever coordinate the caller stripped to.

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn a_quote_under_a_closed_item_does_not_inherit_its_column() {
    // The blank closes the item, and `>   [r]: /u` is an indented line inside a
    // quote - not a definition at the item's column. It stays literal.
    let html = squash(&to_html("- a\n\n>   [r]: /u\n\nsee [t][r]"));

    assert!(
        html.contains("[r]: /u"),
        "the line is still on the page: {html}"
    );
    assert!(
        html.contains("see [t][r]"),
        "the reference stays literal: {html}"
    );
}

#[test]
fn a_deeper_quote_does_not_inherit_a_quoted_items_column() {
    let html = squash(&to_html("> - a\n>\n> >   [r]: /u\n\nsee [t][r]"));

    assert!(html.contains("[r]: /u"), "{html}");
    assert!(html.contains("see [t][r]"), "{html}");
}

#[test]
fn an_items_own_continuation_still_defines_across_a_quote_it_contains() {
    // The quote here is the item's own block, written AT its content column, so
    // the column survives it and the definition after it is the item's.
    let html = squash(&to_html("- item\n  > quoted\n  [r]: /u\n\nsee [t][r]"));

    assert!(html.contains("<a href=\"/u\">t</a>"), "{html}");
    assert!(
        !html.contains("[r]: /u"),
        "the definition renders nothing: {html}"
    );
}

#[test]
fn the_same_holds_for_a_quoted_item() {
    let html = squash(&to_html(
        "> - item\n>   > quoted\n>   [r]: /u\n\nsee [t][r]",
    ));

    assert!(html.contains("<a href=\"/u\">t</a>"), "{html}");
    assert!(!html.contains("[r]: /u"), "{html}");
}

#[test]
fn a_definition_at_a_quoted_items_column_still_registers() {
    // The shape #587 added, unchanged: columns measured inside a quote are what
    // makes this work at all.
    let html = squash(&to_html("> - a\n>   [r]: /u\n\nsee [t][r]"));

    assert!(html.contains("<a href=\"/u\">t</a>"), "{html}");
}

#[test]
fn the_top_level_shapes_are_unchanged() {
    assert!(squash(&to_html("- a\n  [r]: /u\n\nsee [t][r]")).contains("<a href=\"/u\">t</a>"));
    assert!(squash(&to_html("> [r]: /u\n\nsee [t][r]")).contains("<a href=\"/u\">t</a>"));
    // Below every column it is text, and defines nothing.
    assert!(squash(&to_html("text\n  [r]: /u\n\nsee [t][r]")).contains("see [t][r]"));
}
