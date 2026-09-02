//! A COMMENT AT A CONTAINER'S MINIMUM COLUMN DOES NOT LICENSE A REBASE of the
//! over-indented opener under it (markup-carve/carve-rs#1517).
//!
//! `rebase_overindented_blocks` treats a line AT the container's minimum column
//! as ownership evidence: the scan is back in the container's coordinate
//! system, so the next below-column opener was authored there and is rebased to
//! column 0. A comment renders nothing at any column (PART 9 §24 C3), so it is
//! not that evidence, and the line under it is still whatever the line above
//! left open.
//!
//! ORACLE: the executable spec at carve `2775b6df` (spec MAIN), not the pinned
//! `tests/spec` revision. The pin predates markup-carve/carve#1902, whose
//! oracle bug is in the comment column exemption this file is about; measuring
//! the comment family against the pin answers a question the spec no longer
//! asks. Every expectation below is that oracle's own output.

use carve::{to_html, to_html_with_options, Options};

fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade.trim().to_string()
}

#[test]
fn the_reported_document_keeps_the_heading_as_text() {
    assert_eq!(
        both_paths("- x\n  %% x\n # h\n"),
        "<ul>\n  <li>x\n    # h\n  </li>\n</ul>",
    );
}

#[test]
fn every_opener_kind_under_the_comment_stays_text() {
    // The kind does not matter - the column does. A thematic break rendered an
    // `<hr>`, a quote a `<blockquote>` and a table a whole `<table>`, each from
    // a line that reaches nothing.
    for (line, folded) in [
        (" ---", "—"),
        (" > q", "&gt; q"),
        (" | a |", "| a |"),
        (" ::: note", "::: note"),
    ] {
        let src = format!("- x\n  %% x\n{line}\n");
        assert_eq!(
            both_paths(&src),
            format!("<ul>\n  <li>x\n    {folded}\n  </li>\n</ul>"),
            "{src:?}",
        );
    }
}

#[test]
fn a_line_below_the_column_without_a_comment_is_unchanged() {
    // THE CONTROL FOR THE COMMENT. The same document with the comment removed
    // folded correctly before this fix and still does - carve-rs#1509 measured
    // that at 0 across all 306 (prefix, column) pairs.
    assert_eq!(both_paths("- x\n # h\n"), "<ul>\n  <li>x\n# h</li>\n</ul>",);
}

#[test]
fn an_opener_at_the_content_column_still_opens() {
    // THE CONTROL FOR THE COLUMN. A line that DOES reach the item is the item's
    // block, comment above it or not.
    assert_eq!(
        both_paths("- x\n  %% x\n  # h\n"),
        "<ul>\n  <li>x\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>",
    );
}

#[test]
fn an_opener_between_two_content_columns_still_opens_in_the_outer_item() {
    // THE CONTROL FOR carve-rs#1509. Column 3 reaches the outer item and falls
    // short of the inner one, so it opens there - a comment above it does not
    // take that away.
    assert_eq!(
        both_paths("- - x\n    %% c\n   # h\n"),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <ul>\n",
            "      <li>x</li>\n",
            "    </ul>\n",
            "    <h1 id=\"h\">h</h1>\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn a_plain_line_at_the_minimum_column_still_licenses_the_rebase() {
    // THE CONTROL FOR THE FLAG ITSELF (carve-rs#1415). Only a comment stops
    // being evidence; an ordinary block at the body's own column still is, and
    // the opener below it still rebases into that body.
    for above in ["> q", "# a", "| A |"] {
        let output = both_paths(&format!("[^a]: {above}\n      # h\n\nsee[^a]\n"));
        assert!(
            output.contains("<h1 id=\"h\">h</h1>"),
            "{above:?}: {output}"
        );
        assert!(!output.contains("<p># h"), "{above:?}: {output}");
    }
}
