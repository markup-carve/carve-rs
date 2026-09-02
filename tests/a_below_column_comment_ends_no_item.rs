//! A COMMENT BELOW A NESTED ITEM'S CONTENT COLUMN ENDS NOTHING
//! (markup-carve/carve-rs#1516).
//!
//! PART 9 §10 I5's first exception: a comment is column-exempt (§24 C3), so
//! below a container's content column it is still invisible and still closes
//! the paragraph. This engine applied the "still invisible" half and read the
//! other half backwards: below the column the comment adds no block, so it
//! cannot end a container it never reached - which is what §24 C3's "AT OR
//! PAST" reserves for a line that does. The line under it then belongs to the
//! innermost open item, not to the outer one.
//!
//! THE ENGINE ALREADY HAD THE AT-OR-PAST HALF RIGHT, and every assertion below
//! that ends the list is a control for it: at the column the comment IS a block
//! of the item, it ends there, and `tail` reparses at document level.
//!
//! ORACLE VERSION. Measured against the executable spec at carve `f59cc880`,
//! which is PAST this repo's pinned `tests/spec` (`86569bd`) - deliberately.
//! markup-carve/carve#1902 fixed the oracle's own quote host to apply the
//! comment column exemption, and before that fix the pinned oracle folds `%% c`
//! into a quoted paragraph as literal text and cannot answer this family at all
//! (517 of 572 documents disagree with every engine, not 21). The pinned corpus
//! constrains none of these documents: the whole suite passes unchanged.

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
fn the_reported_document_keeps_the_line_in_the_innermost_item() {
    assert_eq!(
        both_paths("- a\n  - x\n%% c\ntail\n"),
        concat!(
            "<ul>\n",
            "  <li>a\n",
            "    <ul>\n",
            "      <li>x\n",
            "        tail\n",
            "      </li>\n",
            "    </ul>\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn the_marker_lead_spelling_answers_the_same_way() {
    // ONE STRUCTURE, ONE ANSWER. `- - a` and `- x` / `  - a` are the same two
    // items, and before this branch the engine gave them different answers -
    // the marker-lead one ended the whole list and made `tail` a document
    // paragraph.
    assert_eq!(
        both_paths("- - a\n %% c\ntail\n"),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <ul>\n",
            "      <li>a\n",
            "        tail\n",
            "      </li>\n",
            "    </ul>\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn both_spellings_agree_at_every_below_column_column() {
    // The band is every column below the INNERMOST content column. Under two
    // list levels that is columns 0 and 1, and both must answer alike in both
    // spellings.
    for column in 0..=1usize {
        let pad = " ".repeat(column);
        let ladder = both_paths(&format!("- x\n  - a\n{pad}%% c\ntail\n"));
        assert!(
            ladder.contains("<li>a\n        tail\n      </li>"),
            "column {column} ladder: {ladder}"
        );
        let marker_lead = both_paths(&format!("- - a\n{pad}%% c\ntail\n"));
        assert!(
            marker_lead.contains("<li>a\n        tail\n      </li>"),
            "column {column} marker-lead: {marker_lead}"
        );
    }
}

#[test]
fn a_deeper_container_under_the_item_keeps_the_line_too() {
    // The innermost open paragraph is the quote's, and the comment reached
    // neither it nor the item holding it.
    assert_eq!(
        both_paths("- x\n  - x\n    > a\n%% c\ntail\n"),
        concat!(
            "<ul>\n",
            "  <li>x\n",
            "    <ul>\n",
            "      <li>x\n",
            "        <blockquote><p>a</p></blockquote>\n",
            "        tail\n",
            "      </li>\n",
            "    </ul>\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn a_comment_at_a_live_content_column_still_ends_the_item() {
    // THE CONTROLS, and the half the engine already had. At column 2 the
    // comment reaches the OUTER item and is a block of it; at column 4 it
    // reaches the inner one. Either way the item ends and `tail` reparses at
    // document level.
    for src in ["- - a\n  %% c\ntail\n", "- a\n  - x\n    %% c\ntail\n"] {
        let html = both_paths(src);
        assert!(html.ends_with("<p>tail</p>"), "{src:?}: {html}");
    }
}

#[test]
fn a_single_item_was_right_before_and_after() {
    // With one level there is no column below the innermost one but zero, and
    // the engine already folded there.
    assert_eq!(
        both_paths("- a\n%% c\ntail\n"),
        "<ul>\n  <li>a\n    tail\n  </li>\n</ul>",
    );
}

#[test]
fn a_comment_fence_at_column_zero_still_ends_the_list() {
    // NOT THE SAME LINE. A `%%%` opener is a fence spelling, and the executable
    // spec ends the list here where the `%%` line form folds - the one place
    // the two spellings part, which markup-carve/carve#1907 recorded rather
    // than decided and markup-carve/carve#1903 asks about. This branch does not
    // touch it, and the assertion is here so a later one cannot move it by
    // accident.
    let html = both_paths("- a\n  - x\n%%% c\ntail\n");
    assert!(html.ends_with("<p>tail</p>"), "{html}");
}
