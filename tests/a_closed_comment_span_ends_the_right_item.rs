//! A CLOSED COMMENT SPAN ENDS THE SAME ITEM THE DEGRADED SPELLING DOES
//! (markup-carve/carve-rs#1531).
//!
//! A span that closes renders nothing, exactly as a fence with no closer
//! degrades to a line comment and renders nothing (PART 9 §28). Neither leaves
//! a paragraph open, so below the content column there is nothing for a line to
//! continue and the frames under it end. #1530 taught the collector that for the
//! DEGRADED spelling; the closed one sets `comment_fence` instead and consulted
//! neither the column nor the carried `reached` flag, so the line below it
//! stayed one item too deep.
//!
//! ORACLE: the executable spec (`scripts/spec/layout.mjs` + `html.mjs`) at carve
//! `2f654da9`, spec main.
//!
//! NOT IN SCOPE: a flush-left LIST MARKER below the span. It is the
//! below-column-marker family of markup-carve/carve-rs#1514 and is unmoved here
//! - `a_marker_follower_is_1514_and_does_not_move` pins that.

use carve::{to_html, to_html_with_options, Options};

/// The library facade and the position-tracking path must agree - the #908
/// guard. It is not decoration here: the two collectors this fix touches ARE
/// that split, and patching only the mapped one left them disagreeing.
fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

fn assert_html(src: &str, expected: &str) {
    let normalize = |html: &str| {
        html.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace(" <", "<")
    };
    assert_eq!(
        normalize(&both_paths(src)),
        normalize(expected),
        "on {src:?}"
    );
}

const INNER_ENDS: &str = "<ul><li><ul><li>x</li></ul> # h</li></ul>";

/// The reported document. The span closes at the inner item's content column
/// and ` # h` reaches nothing, so the inner item ends and the line is the OUTER
/// item's text.
#[test]
fn the_reported_document_ends_the_inner_item() {
    assert_html("- - x\n    %%% x\n    %%%\n # h\n", INNER_ENDS);
}

/// THE CONTROL THAT NAMES THE DEFECT: the degraded spelling of the same
/// document already answered this way, and must not move.
#[test]
fn the_degraded_spelling_answers_the_same_way() {
    assert_html("- - x\n    %%% x\n # h\n", INNER_ENDS);
}

/// A WIDER RUN degrades and closes the same - length is not what decides it.
#[test]
fn a_wider_run_answers_the_same_way() {
    assert_html("- - x\n    %%%% x\n    %%%%\n # h\n", INNER_ENDS);
}

/// AT DEPTH ONE there is no frame under the item, so the line folds into it.
/// A fix that ended every frame would move this.
#[test]
fn depth_one_still_folds() {
    assert_html("- x\n  %%% x\n  %%%\n # h\n", "<ul><li>x # h</li></ul>");
}

/// AT DEPTH THREE every frame the line did not reach ends.
#[test]
fn depth_three_ends_every_frame_below() {
    assert_html(
        "- - - x\n      %%% x\n      %%%\n # h\n",
        "<ul><li><ul><li><ul><li>x</li></ul></li></ul> # h</li></ul>",
    );
}

/// A LINE THAT REACHES THE OUTER ITEM'S COLUMN is that item's content, so it
/// is a heading there rather than text. This is what the carried `reached`
/// flag answers and local column arithmetic cannot.
#[test]
fn a_line_reaching_the_outer_column_is_a_heading_there() {
    assert_html(
        "- - x\n    %%% x\n    %%%\n   # h\n",
        "<ul><li><ul><li>x</li></ul><h1 id=\"h\">h</h1></li></ul>",
    );
}

/// A SPAN BELOW EVERY COLUMN reached no frame at all, so it ends none of them
/// and the line folds where it was already going.
#[test]
fn a_span_below_every_column_ends_nothing() {
    assert_html(
        "- - x\n %%% x\n %%%\n # h\n",
        "<ul><li><ul><li>x # h</li></ul></li></ul>",
    );
}

/// A THEMATIC BREAK follower answers the same way.
#[test]
fn a_thematic_break_follower_answers_the_same_way() {
    assert_html(
        "- - x\n    %%% x\n    %%%\n ---\n",
        "<ul><li><ul><li>x</li></ul> —</li></ul>",
    );
}

/// A QUOTE follower answers the same way.
#[test]
fn a_quote_follower_answers_the_same_way() {
    assert_html(
        "- - x\n    %%% x\n    %%%\n > q\n",
        "<ul><li><ul><li>x</li></ul> &gt; q</li></ul>",
    );
}

/// A PLAIN follower already landed correctly before this change - the tell
/// #1517 and #1518 had, and the row that says the fix did not simply move
/// everything.
#[test]
fn a_plain_follower_does_not_move() {
    assert_html(
        "- - x\n    %%% x\n    %%%\n p\n",
        "<ul><li><ul><li>x</li></ul> p</li></ul>",
    );
}

/// A SECOND SPAN LEFT OPEN after the first closed: the chunk now ends
/// DEGRADED, so the degraded test answers it and the two compose to one break.
#[test]
fn a_second_span_left_open_still_ends_it() {
    assert_html("- - x\n    %%% x\n    %%%\n    %%% y\n # h\n", INNER_ENDS);
}

/// OUT OF SCOPE, PINNED SO IT CANNOT MOVE SILENTLY: a flush-left list marker
/// below the span is markup-carve/carve-rs#1514's family. The oracle makes it a
/// SIBLING of the inner item; this engine still opens a fresh list, before and
/// after.
#[test]
fn a_marker_follower_is_1514_and_does_not_move() {
    assert_html(
        "- - x\n    %%% x\n    %%%\n - m\n",
        "<ul><li><ul><li>x</li></ul><ul><li>m</li></ul></li></ul>",
    );
}

/// THE SPAN HAS TO REACH THE FRAME IT ENDS. Written below every content column
/// it reached none of them, so it ends none - and the line under it folds where
/// it was already going. Without the column test the span ends frames it never
/// touched.
#[test]
fn a_span_that_reached_nothing_ends_nothing() {
    assert_html(
        "- - - x\n %%% x\n %%%\n # h\n",
        "<ul><li><ul><li><ul><li>x # h</li></ul></li></ul></li></ul>",
    );
}

/// THE CARRIED FLAG, NOT THE LOCAL COLUMN. `   - m` is below the inner item's
/// content column but REACHES the outer item, so the inner item is still open
/// under it and the marker opens a sublist there. Breaking on `indent <
/// strip_cols` instead ends the inner item and starts a fresh list - the
/// ambiguity `MappedSource::reached` exists for.
#[test]
fn a_line_reaching_an_ancestor_keeps_the_frame_open() {
    assert_html(
        "- - x\n    %%% x\n    %%%\n   - m\n",
        "<ul><li><ul><li>x<ul><li>m</li></ul></li></ul></li></ul>",
    );
}
