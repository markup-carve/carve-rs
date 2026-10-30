//! A marked description reaches a term in a quoted item. An unmarked lazy
//! `:` has no column inside the quote, so it leaves the term (carve#4300).
//!
//! THREE STATES ARE EXCLUDED, and each has its own row below. A description
//! body already OPEN takes the line as its own lazy continuation rather than a
//! second entry. A line BELOW the innermost container's content column reached
//! no container at all and is already lazy text wherever it sits. And a BLOCK
//! written inside the entry closes it, so there is no open half to attach to.
//!
//! ONE SHAPE, DELIBERATELY. Every lazy line has this much in common, and
//! handing them all down flush is the general case markup-carve/carve-js#1609
//! tracks separately - it moves fence- and marker-shaped lines off the oracle's
//! answer. The rows named `keeps_its_column` below hold that line.
//!
//! ORACLE: the executable spec (`scripts/spec/layout.mjs` + `html.mjs`) at carve
//! `35148309`. The pinned submodule is `95fc3a04`; the two agree on every
//! document here, so the pin/main split does not reach this family.

use carve::{to_html, to_html_with_options, Options};

/// The library facade and the position-tracking path must agree - the #908
/// guard.
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

// ---------------------------------------------------------------------------
// THE BAND: a `:`-shaped lazy line under a quoted item is the description.
// ---------------------------------------------------------------------------

/// The reported unmarked line folds into the term.
#[test]
fn the_reported_document_reads_the_body() {
    assert_html(
        "> - :: t\n    :  a\ntail\n",
        "<blockquote><ul><li><dl><dt>t</dt></dl></li></ul></blockquote><p>: a tail</p>",
    );
}

/// The control that localizes it: drop the quote, keep everything else, and the
/// same engine already read the body. The quote prefix was the difference.
#[test]
fn the_unquoted_control_is_unchanged() {
    assert_html(
        "- :: t\n  :  a\ntail\n",
        "<ul><li><dl><dt>t</dt><dd>a tail</dd></dl></li></ul>",
    );
}

/// A line carrying `>` has a quoted column and opens the description.
#[test]
fn a_line_carrying_its_own_marker_answers_the_same_way() {
    assert_html(
        "> - :: t\n>   :  a\ntail\n",
        "<blockquote><ul><li><dl><dt>t</dt><dd>a tail</dd></dl></li></ul></blockquote>",
    );
}

/// Two quote markers keep the same unmarked term fold.
#[test]
fn a_deeper_quote_answers_the_same_way() {
    assert_html(
        "> > - :: t\n      :  a\ntail\n",
        "<blockquote><blockquote><ul><li><dl><dt>t</dt></dl></li></ul></blockquote></blockquote><p>: a tail</p>",
    );
}

/// Widening the unmarked separator preserves its term owner.
#[test]
fn a_wide_separator_answers_the_same_way() {
    assert_html(
        "> - :: t\n    :   a\ntail\n",
        "<blockquote><ul><li><dl><dt>t</dt></dl></li></ul></blockquote><p>: a tail</p>",
    );
}

// ---------------------------------------------------------------------------
// THE CONTROLS. Each of these fails a fix that overshoots.
// ---------------------------------------------------------------------------

/// AN OPEN DESCRIPTION BODY TAKES THE LINE. With a body already open the same
/// line is that body's own lazy continuation rather than a second entry - the
/// state carve-js#1608 had to exclude, and 300 documents of the sweep move on
/// it here.
#[test]
fn an_open_description_body_takes_the_line() {
    assert_html(
        "> - :: t\n>   :  d\n   :  a\ntail\n",
        "<blockquote><ul><li><dl><dt>t</dt><dd>d : a tail</dd></dl></li></ul></blockquote>",
    );
}

/// BELOW THE INNERMOST CONTENT COLUMN the line reached no container at all, so
/// it is already lazy text wherever it sits and moving it to column 0 would
/// change its classification rather than restore it.
#[test]
fn a_line_below_the_content_column_still_folds_as_text() {
    assert_closed_leaf_fixture("a_line_below_the_content_column_still_folds_as_text");
}

/// A BLOCK WRITTEN INSIDE THE ENTRY CLOSES IT, so there is no open half for the
/// `:` to attach to and the line is ordinary text.
#[test]
fn a_block_between_them_closes_the_entry() {
    assert_closed_leaf_fixture("a_block_between_them_closes_the_entry");
}

/// THE SAME RESET, WHERE ONLY THE RESET CAN ANSWER IT. Above, the entry was on
/// its TERM half, which the open-description gate lets through anyway; here a
/// description body is open when the block arrives, and the line REACHES the
/// item's content column. Without the reset the half still reads
/// `description`, the line is taken as that body's continuation, and it folds
/// into the item instead of leaving it - 102 documents of the second sweep.
#[test]
fn a_block_closes_an_open_description_body_too() {
    assert_closed_leaf_fixture("a_block_closes_an_open_description_body_too");
}

/// NO ENTRY OPEN AT ALL: a `:` with no term above it opens nothing and folds.
#[test]
fn no_entry_open_at_all_folds_as_text() {
    assert_html(
        "> - x\n    :  a\ntail\n",
        "<blockquote><ul><li>x : a tail</li></ul></blockquote>",
    );
}

/// A `: ` separator that is not a description marker is not this band either.
#[test]
fn a_narrow_separator_is_not_a_description_marker() {
    assert_html(
        "> - : t\n    : a\ntail\n",
        "<blockquote><ul><li>: t : a tail</li></ul></blockquote>",
    );
}

/// ONE SHAPE ONLY - a FENCE-shaped lazy line keeps its column. Handing every
/// lazy line down flush is the general case carve-js#1609 tracks separately.
#[test]
fn a_fence_shaped_lazy_line_keeps_its_column() {
    assert_html(
        "> - x\n    ``` r\ntail\n",
        "<blockquote><ul><li>x</li></ul></blockquote><pre><code class=\"language-r\">tail\n</code></pre>",
    );
}

/// The same limit for a TERM-shaped lazy line: `::` is not the description
/// marker this band is about, and it stays where it was written.
#[test]
fn a_term_shaped_unmarked_line_folds_into_the_open_term() {
    assert_html(
        "> - :: t\n    :: u\ntail\n",
        "<blockquote><ul><li><dl><dt>t</dt></dl></li></ul></blockquote><dl><dt>u tail</dt></dl>",
    );
}

// ---------------------------------------------------------------------------
// TWO CONDITIONS THE FIRST TABLE LEFT UNPINNED.
// ---------------------------------------------------------------------------

/// A BLANK DOES NOT CLOSE THE ENTRY. It only loosens the list, so a paragraph
/// reopened at the body's own column is still that body's, and the `:` under it
/// is its continuation rather than a second entry. The scan reset its half on a
/// blank at first, which moved 12 documents of this band off the oracle.
#[test]
fn a_blank_does_not_close_the_open_description() {
    assert_html(
        "> - :: t\n>   :  d\n>\n>       p\n    :  a\ntail\n",
        "<blockquote><ul><li><dl><dt>t</dt><dd><p>d</p><p>p : a tail</p></dd></dl></li>\
         </ul></blockquote>",
    );
}

/// The control that says it is the BODY's column doing the work and not the
/// blank: reopen the paragraph at the ITEM's column instead and the entry is
/// behind it, so the `:` is ordinary text either way.
#[test]
fn a_paragraph_reopened_at_the_item_column_leaves_the_entry() {
    assert_html(
        "> - :: t\n>   :  d\n>\n>   q\n    :  a\ntail\n",
        "<blockquote><ul><li><dl><dt>t</dt><dd>d</dd></dl><p>q : a tail</p></li></ul></blockquote>",
    );
}

/// The folded term text keeps the unmarked line's source column.
#[test]
fn the_stripped_columns_stay_in_the_position_map() {
    let src = "> - :: t\n    :  a\ntail\n";
    let json = carve::to_json_with_options(src, &Options::default().with_positions(true));
    assert!(
        json.contains(
            "\"value\":\":  a\",\"pos\":{\"startLine\":2,\"endLine\":2,\"startColumn\":5"
        ),
        "the folded term text must retain the authored source column; got {json}"
    );
}

fn assert_closed_leaf_fixture(id: &str) {
    let rows: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/quoted_description_closed_leaf_wave.json"
    ))
    .unwrap();
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap();
    assert_html(
        row["source"].as_str().unwrap(),
        row["html"].as_str().unwrap(),
    );
}
