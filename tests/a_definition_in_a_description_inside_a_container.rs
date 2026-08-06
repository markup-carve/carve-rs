//! A definition written in a description is collected inside a container too
//! (markup-carve/carve#840).
//!
//! Collecting empties the `dd` (markup-carve/carve#801) and hoists the node to
//! the document - PART 12 section 10: "a definition authored inside a block
//! quote or a list item is a child of the DOCUMENT". Inside a block quote or a
//! list item this engine did neither: the line stayed in the `dd` as content,
//! so a reference to it did not resolve and the entry kept a trace the top
//! level does not.
//!
//! The prepass strips container prefixes before matching, but it asked "is this
//! line a description?" of the RAW previous line, so `> :: term` did not read
//! as a term and the `:  ` marker below it was never stripped. A div has no
//! per-line prefix, which is why that container always worked.

use carve::to_html;

const IN_QUOTE: &str = "> :: term\n> :  [r]: /u\n>\n> see [t][r]\n";
const IN_ITEM: &str = "- :: term\n  :  [r]: /u\n\nsee [t][r]\n";

#[test]
fn the_reference_resolves_from_a_block_quote() {
    let html = to_html(IN_QUOTE);

    assert!(html.contains("href=\"/u\""), "{html}");
}

#[test]
fn the_reference_resolves_from_a_list_item() {
    let html = to_html(IN_ITEM);

    assert!(html.contains("href=\"/u\""), "{html}");
}

#[test]
fn the_description_is_emptied_the_way_it_is_at_top_level() {
    // The other half of the collection contract, asserted apart from
    // resolution: leaving the line in the `dd` also resolves nothing, so one
    // assertion cannot tell the two failures apart.
    let html = to_html(IN_QUOTE);

    assert!(html.contains("<dd></dd>"), "{html}");
    assert!(!html.contains("[r]: /u"), "{html}");
}

#[test]
fn top_level_and_a_div_still_work() {
    // The controls: the shapes that already passed must keep passing.
    assert!(to_html(":: term\n:  [r]: /u\n\nsee [t][r]\n").contains("href=\"/u\""));
    assert!(to_html("::: note\n:: term\n:  [r]: /u\n:::\n\nsee [t][r]\n").contains("href=\"/u\""));
}

#[test]
fn a_description_line_with_no_term_above_it_is_not_one() {
    // The boundary the gate exists for (corpus
    // 216-a-description-line-needs-a-term-above-it): a lone `:  ` line is not a
    // description, so what follows its marker is not a definition and the
    // reference stays literal.
    let html = to_html(":  [r]: /u\n\nsee [t][r]\n");

    assert!(!html.contains("href=\"/u\""), "{html}");
}
