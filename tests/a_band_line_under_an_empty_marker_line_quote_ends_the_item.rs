//! AN EMPTY QUOTE ON THE MARKER LINE ENDS THE ITEM
//! (markup-carve/carve-rs#2096, reduction 01).
//!
//! A quote has no closer, so an empty one written as an item's marker-line
//! content is finished where it stands. PART 1 S4's otherwise has then already
//! ended the item, and a line in the band between the list's base and the item's
//! content column has nothing here to continue: it belongs to the document.
//!
//! The band is the whole condition. At column 0 the list ends anyway, at the
//! content column the line reaches the item, and `. > a` folds because a
//! paragraph really is open.
//!
//! Expectations are the executable spec's output, run per document against
//! `scripts/spec/layout.mjs` plus `scripts/spec/html.mjs` at markup-carve/carve
//! 774eb404, and cross-read against carve-js 6c4b882b1 and carve-php 4efa406bc,
//! which agree on every document here.

use carve::{to_html, to_html_with_options, Options};

fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

#[test]
fn the_reported_reduction_leaves_the_band_line_at_document_level() {
    assert_eq!(
        both_paths(". >\n %\n").trim(),
        concat!(
            "<ol>\n",
            "  <li>\n",
            "    <blockquote>\n",
            "\n",
            "    </blockquote>\n",
            "  </li>\n",
            "</ol>\n",
            "<p>%</p>",
        ),
    );
}

#[test]
fn every_marker_kind_and_every_band_line_reads_the_same_way() {
    // The list base is 0 in each, so the band is column 1 for `- ` and `. `,
    // and columns 1 and 2 for `10. `.
    for src in [
        "- >\n %\n",
        "* >\n z\n",
        ". >\n z\n",
        "1. >\n z\n",
        "10. >\n z\n",
        "10. >\n  z\n",
    ] {
        let html = both_paths(src);
        assert!(
            html.contains("</blockquote>\n  </li>\n</"),
            "the band line stayed in the item: {src:?}: {html}"
        );
        assert!(
            html.trim_end().ends_with("</p>"),
            "the band line did not reach the document: {src:?}: {html}"
        );
    }
}

#[test]
fn a_paragraph_inside_the_quote_still_folds() {
    // THE CONTROL the band condition turns on: `> a` leaves a paragraph open,
    // so the band line is its lazy continuation and the item keeps it.
    assert_eq!(
        both_paths(". > a\n z\n").trim(),
        concat!(
            "<ol>\n",
            "  <li>\n",
            "    <blockquote><p>a\n",
            "z</p></blockquote>\n",
            "  </li>\n",
            "</ol>",
        ),
    );
}

#[test]
fn a_fence_or_a_container_inside_the_quote_still_takes_the_band_line() {
    // THE OTHER CONTROL, and the reason the predicate is not "holds a
    // paragraph": a fence or a colon container written inside the quote is open
    // on one line, and the band line below it is its payload. Asked about the
    // paragraph alone, this arm refused 28 of the sweep's shapes.
    let fence = both_paths("- > ```\n z\n");
    assert!(fence.contains("<pre><code>"), "{fence}");
    assert!(fence.contains('z'), "the fence lost its payload: {fence}");

    let container = both_paths("- > :::\n z\n");
    assert!(container.contains('z'), "{container}");
}

#[test]
fn the_columns_on_either_side_of_the_band_are_unmoved() {
    // At the content column the line REACHES the item, so it is the item's own
    // block, and at column 0 the list ends through the ordinary dedent. Both
    // already agreed with the spec, and a fix reaching past the band would move
    // them.
    let at_column = both_paths(". >\n  z\n");
    assert!(
        at_column.contains("</blockquote>\n    z\n  </li>"),
        "{at_column}"
    );

    let flush = both_paths(". >\nz\n");
    assert!(flush.trim_end().ends_with("<p>z</p>"), "{flush}");
    assert!(!flush.contains("z</li>"), "{flush}");
}

#[test]
fn a_marker_line_that_is_not_a_quote_at_all_is_untouched() {
    // `>>` has no space after the inner marker, so it opens no quote and is
    // paragraph text - the band line folds into it. This is the shape that makes
    // the predicate a question about what the marker line leaves open rather
    // than about the `>` character.
    let text = both_paths(". >>\n z\n");
    assert_eq!(
        text.trim(),
        concat!("<ol>\n", "  <li>&gt;&gt;\n", "z</li>\n", "</ol>"),
    );
}
