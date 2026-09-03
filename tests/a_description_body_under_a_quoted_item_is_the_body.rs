//! A DESCRIPTION BODY UNDER A QUOTED ITEM IS THE BODY, NOT THE TERM
//! (markup-carve/carve-rs#1526; carve-js landed the same fix as
//! markup-carve/carve-js#1608).
//!
//! PART 0 LAZY CONTINUATION: a line carrying no `>` still continues the quote by
//! folding into the innermost open paragraph, and it is NOT the quote's content
//! at any column - the quote is reached by its marker and never by a column, so
//! the line's indentation inside the quote body means nothing. The collectors
//! below read the quote's inner lines BY COLUMN anyway: `> - :: t` puts the
//! item's content column at 4, a `:  a` written there reached it, and the
//! content-column arm dedented by the item's own column and left the rest as
//! leading indent. An indented `:` is no longer a description marker, so the
//! body folded into the TERM - while the same document WITHOUT the quote read
//! the body. The quote prefix was the whole difference.
//!
//! The quote collector now hands such a line down flush, which is PART 9 §24
//! C3's LENIENT def-list entry: a `:` attaches a fresh description to an open
//! term from at or below column 0.
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

/// The reported document. `> - :: t` puts the item's content column at 4 and
/// `:  a` is written there.
#[test]
fn the_reported_document_reads_the_body() {
    assert_html(
        "> - :: t\n    :  a\ntail\n",
        "<blockquote><ul><li><dl><dt>t</dt><dd>a tail</dd></dl></li></ul></blockquote>",
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

/// A line that carries its OWN `>` is the quote's content and was never in this
/// band. It answers the same way, which is what says the two spellings agree.
#[test]
fn a_line_carrying_its_own_marker_answers_the_same_way() {
    assert_html(
        "> - :: t\n>   :  a\ntail\n",
        "<blockquote><ul><li><dl><dt>t</dt><dd>a tail</dd></dl></li></ul></blockquote>",
    );
}

/// Two quote markers. The OUTER quote declines the line - at its level the
/// item is behind another `>` and no content column is visible - and the
/// INNER one, whose lines carry no further marker, strips it to the same
/// place. That is why this reader needs no quote-marker walk of its own.
#[test]
fn a_deeper_quote_answers_the_same_way() {
    assert_html(
        "> > - :: t\n      :  a\ntail\n",
        "<blockquote><blockquote><ul><li><dl><dt>t</dt><dd>a tail</dd></dl></li>\
         </ul></blockquote></blockquote>",
    );
}

/// A WIDER separator moves the body's column without moving the rule.
#[test]
fn a_wide_separator_answers_the_same_way() {
    assert_html(
        "> - :: t\n    :   a\ntail\n",
        "<blockquote><ul><li><dl><dt>t</dt><dd>a tail</dd></dl></li></ul></blockquote>",
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
    assert_html(
        "> - - :: t\n>     # h\n :  a\ntail\n",
        "<blockquote><ul><li><ul><li><dl><dt>t</dt></dl><h1 id=\"h\">h</h1></li></ul> \
         : a tail</li></ul></blockquote>",
    );
}

/// A BLOCK WRITTEN INSIDE THE ENTRY CLOSES IT, so there is no open half for the
/// `:` to attach to and the line is ordinary text.
#[test]
fn a_block_between_them_closes_the_entry() {
    assert_html(
        "> - :: t\n>   # h\n   :  a\ntail\n",
        "<blockquote><ul><li><dl><dt>t</dt></dl><h1 id=\"h\">h</h1></li></ul>\
         <p>: a tail</p></blockquote>",
    );
}

/// THE SAME RESET, WHERE ONLY THE RESET CAN ANSWER IT. Above, the entry was on
/// its TERM half, which the open-description gate lets through anyway; here a
/// description body is open when the block arrives, and the line REACHES the
/// item's content column. Without the reset the half still reads
/// `description`, the line is taken as that body's continuation, and it folds
/// into the item instead of leaving it - 102 documents of the second sweep.
#[test]
fn a_block_closes_an_open_description_body_too() {
    assert_html(
        "> - :: t\n>   :  d\n>   # h\n    :  a\ntail\n",
        "<blockquote><ul><li><dl><dt>t</dt><dd>d</dd></dl><h1 id=\"h\">h</h1></li></ul>\
         <p>:  a tail</p></blockquote>",
    );
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
        "<blockquote><ul><li>x<code> r tail</code></li></ul></blockquote>",
    );
}

/// The same limit for a TERM-shaped lazy line: `::` is not the description
/// marker this band is about, and it stays where it was written.
#[test]
fn a_term_shaped_lazy_line_keeps_its_column() {
    assert_html(
        "> - :: t\n    :: u\ntail\n",
        "<blockquote><ul><li><dl><dt>t</dt><dt>u tail</dt></dl></li></ul></blockquote>",
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

/// THE STRIPPED COLUMNS STAY IN THE POSITION MAP. Dedenting the line moves its
/// text, so the columns removed have to be added back to what the container
/// recorded as stripped - and NO HTML COMPARISON CAN SEE THIS. Without it the
/// description's text reports column 4 of line 2, four columns early, which is
/// where the `:` sits rather than the body (the #908 shape).
#[test]
fn the_stripped_columns_stay_in_the_position_map() {
    let src = "> - :: t\n    :  a\ntail\n";
    let json = carve::to_json_with_options(src, &Options::default().with_positions(true));
    let flat: String = json.split_whitespace().collect::<Vec<_>>().join("");
    assert!(
        flat.contains("\"value\":\"a\",\"pos\":{\"startLine\":2,\"endLine\":2,\"startColumn\":8"),
        "the description text must report the column it was written at; got {json}"
    );
}
