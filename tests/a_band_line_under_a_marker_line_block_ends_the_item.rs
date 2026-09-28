//! A BAND LINE UNDER A MARKER-LINE BLOCK THAT LEAVES NOTHING OPEN
//! (markup-carve/carve-rs#2120).
//!
//! A heading, a thematic break or a table written as an item's marker-line
//! content leaves the item holding no paragraph, so PART 1 S4's otherwise has
//! already ended the item. A line in the band between the list's base and the
//! item's content column then has nothing here to continue and belongs to the
//! document.
//!
//! This is the sibling of the empty marker-line quote, and the band is the same
//! whole condition: at column 0 the list ends anyway, at the content column the
//! line reaches the item, and a paragraph collected in between takes the band
//! line back as its lazy continuation.
//!
//! The second half is the state the first needed: `item_paragraph_open` was
//! never set on the lead-paragraph path, so an item could answer S4 with the
//! PREVIOUS item's answer.
//!
//! Expectations are the executable spec's output, run per document against
//! `scripts/spec/layout.mjs` plus `scripts/spec/html.mjs` at markup-carve/carve
//! 1f51a01d, and re-read at 66d4ed19, where every document answers the same.

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
        both_paths(". # h\n z\n").trim(),
        concat!(
            "<ol>\n",
            "  <li>\n",
            "    <h1 id=\"h\">h</h1>\n",
            "  </li>\n",
            "</ol>\n",
            "<p>z</p>",
        ),
    );
}

#[test]
fn a_thematic_break_and_a_table_answer_the_same_way() {
    // The other two kinds the ticket names: each leaves the item holding no
    // paragraph, so each ends it at a band line.
    assert_eq!(
        both_paths(". ---\n z\n").trim(),
        concat!(
            "<ol>\n",
            "  <li>\n",
            "    <hr>\n",
            "  </li>\n",
            "</ol>\n",
            "<p>z</p>",
        ),
    );
    assert_eq!(
        both_paths(". | a |\n z\n").trim(),
        concat!(
            "<ol>\n",
            "  <li>\n",
            "    <table>\n",
            "      <tbody>\n",
            "        <tr><td>a</td></tr>\n",
            "      </tbody>\n",
            "    </table>\n",
            "  </li>\n",
            "</ol>\n",
            "<p>z</p>",
        ),
    );
}

#[test]
fn every_marker_width_ends_the_item_across_its_whole_band() {
    // `10. ` has a three-column band, so a fix keyed to column 1 would pass the
    // narrow markers and miss the wide one.
    for src in [
        "- # h\n z\n",
        "* # h\n z\n",
        ". # h\n z\n",
        "1. # h\n z\n",
        "1. # h\n  z\n",
        "10. # h\n z\n",
        "10. # h\n  z\n",
        "10. # h\n   z\n",
    ] {
        let html = both_paths(src);
        assert!(
            html.contains("</h1>\n  </li>\n</"),
            "the band line stayed in the item: {src:?}: {html}"
        );
        assert!(
            html.trim_end().ends_with("<p>z</p>"),
            "the band line did not reach the document: {src:?}: {html}"
        );
    }
}

#[test]
fn a_paragraph_collected_at_the_content_column_still_takes_the_band_line() {
    // THE CONTROL the band condition turns on. The item holds an open paragraph
    // by the time the band line arrives, so the line is its lazy continuation -
    // and the raised collection floor hands that decision to the fold rather
    // than making it by itself.
    assert_eq!(
        both_paths(". # h\n  t\n z\n").trim(),
        concat!(
            "<ol>\n",
            "  <li>\n",
            "    <h1 id=\"h\">h</h1>\n",
            "    t\n",
            "z\n",
            "  </li>\n",
            "</ol>",
        ),
    );
    // A run of band lines folds, not just the first.
    let run = both_paths(". # h\n  t\n z\n y\n");
    assert!(run.contains("t\nz\ny\n"), "{run}");
}

#[test]
fn a_second_block_that_leaves_no_paragraph_ends_the_item_too() {
    // The same question asked of the collected body rather than the marker line.
    for src in [". # h\n  # g\n z\n", ". # h\n  ---\n z\n"] {
        let html = both_paths(src);
        assert!(html.trim_end().ends_with("<p>z</p>"), "{src:?}: {html}");
    }
}

#[test]
fn the_columns_on_either_side_of_the_band_are_unmoved() {
    let at_column = both_paths(". # h\n  z\n");
    assert!(at_column.contains("</h1>\n    z\n  </li>"), "{at_column}");

    let flush = both_paths(". # h\nz\n");
    assert!(flush.trim_end().ends_with("<p>z</p>"), "{flush}");
    assert!(!flush.contains("z</li>"), "{flush}");
}

#[test]
fn a_marker_line_that_leaves_something_open_is_untouched() {
    // The reason the predicate asks what the marker line leaves OPEN rather than
    // whether it holds a paragraph: a code fence is open on one line, and an
    // unterminated colon container is paragraph text that the band line folds
    // into. Both already agreed with the spec.
    assert_eq!(
        both_paths(". ```\n z\n").trim(),
        concat!(
            "<ol>\n",
            "  <li>\n",
            "    <pre><code>\n",
            "</code></pre>\n",
            "  </li>\n",
            "</ol>\n",
            "<p>z</p>",
        ),
    );
    assert_eq!(
        both_paths(". ::: note\n z\n").trim(),
        concat!("<ol>\n", "  <li>::: note\n", "z</li>\n", "</ol>"),
    );
}

#[test]
fn an_item_answers_s4_for_itself_rather_than_with_the_previous_item_s_answer() {
    // `item_paragraph_open` gates the flush-comment exception, and the
    // lead-paragraph path never set it. After a marker-line heading set it
    // false, the plain item below inherited that answer and the comment
    // exception declined, so `tail` left an item that does hold a paragraph.
    assert_eq!(
        both_paths("- # h\n- t\n%% c\ntail\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <h1 id=\"h\">h</h1>\n",
            "  </li>\n",
            "  <li>t\n",
            "    tail\n",
            "  </li>\n",
            "</ul>",
        ),
    );
    // Without the heading above it the same item always answered true, which is
    // why this went unseen.
    let control = both_paths("- t\n- t2\n%% c\ntail\n");
    assert!(control.contains("<li>t2\n    tail\n  </li>"), "{control}");
}
