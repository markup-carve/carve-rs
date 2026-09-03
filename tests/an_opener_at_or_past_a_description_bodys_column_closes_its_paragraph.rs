//! AN OPENER AT OR PAST A DESCRIPTION BODY'S CONTENT COLUMN CLOSES THE BODY'S
//! PARAGRAPH, SO THE FLUSH-LEFT LINE BELOW IT ENDS THE BODY
//! (markup-carve/carve#1911, normative in markup-carve/carve#1917).
//!
//! PART 2's BELOW THE BODY'S COLUMN THE BODY ENDS says the first band is about
//! OPENERS and that a non-opener still folds - but only because it has an open
//! paragraph above it that §10 I2's lazy continuation reaches. The two upper
//! bands decide whether there IS one. An opener AT the body's column and an
//! opener PAST it are both the body's own block content (past it the authored
//! base gives it the body's column 0), so §10 I1 closes the paragraph for a
//! visible opener and §10 I5 closes it for a definition or an attribute block.
//! The two columns therefore answer alike; an answer that moves between them is
//! reading indentation rather than the rule.
//!
//! AN OPENER THAT LEAVES A PARAGRAPH OPEN IS NOT COVERED. A block quote opens
//! inside the `dd`, its own paragraph is still open, and the flush-left line
//! lazily continues THE QUOTE - `a_quote_*_keeps_the_follower` below.
//!
//! ORACLE: the executable spec (`scripts/spec/layout.mjs` + `html.mjs`) at
//! carve `35148309`, spec MAIN. The pinned submodule is `95fc3a04`, which
//! predates carve#1917 and therefore still answers the old way, so main is the
//! only revision that can arbitrate this family. Corpus section 444 arrives
//! with the next pin bump; every expectation below is that oracle's own output.

use carve::{to_html, to_html_with_options, Options};

/// The library facade and the position-tracking path must agree - the #908
/// guard. A layout bug reachable only with positions on is invisible to every
/// CLI test.
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
            .replace("> <", "><")
    };
    assert_eq!(
        normalize(&both_paths(src)),
        normalize(expected),
        "on {src:?}"
    );
}

// ---------------------------------------------------------------------------
// THE BAND: an opener at or past the body's column ends the body.
// ---------------------------------------------------------------------------

/// Corpus 444-3. The reported document: a heading one column past a
/// three-column body. The collector slices at the column and leaves ` # H`,
/// which is not an opener - so the fold question, asked of the AUTHORED lines,
/// reported a paragraph the body does not have.
#[test]
fn a_heading_past_the_column_ends_the_body() {
    assert_html(
        ":: term\n:  definition\n    # H\ntail\n",
        "<dl><dt>term</dt><dd><p>definition</p><h1 id=\"H\">H</h1></dd></dl><p>tail</p>",
    );
}

/// Corpus 444-4, and the half this engine was ALONE on: a heading at the body's
/// OWN content column, inside the band PART 2 already called the body's own
/// block content. `definition_body_takes_the_fold` carried a heading arm for
/// it, on the reading that carve#1280 left that half open; carve#1911 closed it.
#[test]
fn a_heading_at_the_column_ends_the_body() {
    assert_html(
        ":: term\n:  definition\n   # H\ntail\n",
        "<dl><dt>term</dt><dd><p>definition</p><h1 id=\"H\">H</h1></dd></dl><p>tail</p>",
    );
}

/// Corpus 444-5.
#[test]
fn a_thematic_break_past_the_column_ends_the_body() {
    assert_html(
        ":: term\n:  definition\n    ***\ntail\n",
        "<dl><dt>term</dt><dd><p>definition</p><hr></dd></dl><p>tail</p>",
    );
}

/// Corpus 444-6.
#[test]
fn a_table_row_past_the_column_ends_the_body() {
    assert_html(
        ":: term\n:  definition\n    | a |\ntail\n",
        "<dl><dt>term</dt><dd><p>definition</p><table><tbody><tr><td>a</td></tr></tbody></table>\
         </dd></dl><p>tail</p>",
    );
}

/// Corpus 444-7, the §10 I5 half. An attribute block is INVISIBLE, so the
/// rebase declined to move it while a paragraph was open - the guard
/// carve#1809 added for a line BELOW the column. Past the column it is the
/// body's own block content, and only `MappedSource::reached` tells the two
/// apart: the slice leaves both with a single residual column.
#[test]
fn an_attribute_block_past_the_column_ends_the_body() {
    assert_html(
        ":: term\n:  definition\n    {.k}\ntail\n",
        "<dl><dt>term</dt><dd>definition</dd></dl><p>tail</p>",
    );
}

/// Corpus 444-8, the AT-column control for the row above: already right, and it
/// fails a fix that reaches the invisible kinds by column arithmetic alone.
#[test]
fn an_attribute_block_at_the_column_ends_the_body() {
    assert_html(
        ":: term\n:  definition\n   {.k}\ntail\n",
        "<dl><dt>term</dt><dd>definition</dd></dl><p>tail</p>",
    );
}

/// Corpus 444-12: the band moves with the separator, not with a fixed column.
#[test]
fn a_wider_separator_moves_the_band() {
    assert_html(
        ":: term\n:    definition\n      # H\ntail\n",
        "<dl><dt>term</dt><dd><p>definition</p><h1 id=\"H\">H</h1></dd></dl><p>tail</p>",
    );
}

/// Corpus 444-13: the same inside a list item, where the follower returns to
/// the ITEM rather than to the document.
#[test]
fn the_band_inside_a_list_item() {
    assert_html(
        "- intro\n\n  :: term\n  :  definition\n      # H\n  tail\n",
        "<ul><li>intro <dl><dt>term</dt><dd><p>definition</p><h1 id=\"H\">H</h1></dd></dl> \
         tail </li></ul>",
    );
}

/// The same inside a QUOTE. The quote's prefix walk is what carve-rs#1526 is
/// about; the band itself has to answer the same way behind one.
#[test]
fn the_band_inside_a_quote() {
    assert_html(
        "> :: term\n> :  definition\n>    # H\n> tail\n",
        "<blockquote><dl><dt>term</dt><dd><p>definition</p><h1 id=\"H\">H</h1></dd></dl>\
         <p>tail</p></blockquote>",
    );
}

/// Corpus 444-14. The band reaches the line BELOW the ended body, whatever its
/// shape: the body has ended, and at document level column 1 is not column 0,
/// so a definition written there opens nothing and is ordinary paragraph text.
#[test]
fn the_line_below_the_ended_body_opens_nothing() {
    assert_html(
        ":: term\n:  definition\n    # H\n [r]: /url\n",
        "<dl><dt>term</dt><dd><p>definition</p><h1 id=\"H\">H</h1></dd></dl><p>[r]: /url</p>",
    );
}

/// Corpus 444-15, the AT-column control for the row above.
#[test]
fn the_line_below_an_at_column_opener_opens_nothing() {
    assert_html(
        ":: term\n:  definition\n   # H\n [r]: /url\n",
        "<dl><dt>term</dt><dd><p>definition</p><h1 id=\"H\">H</h1></dd></dl><p>[r]: /url</p>",
    );
}

// ---------------------------------------------------------------------------
// THE CONTROLS. Each of these fails a fix that overshoots.
// ---------------------------------------------------------------------------

/// Corpus 444-11, and the carve-out the ruling names outright. A block quote is
/// an opener that leaves a paragraph OPEN: it opens inside the `dd`, its own
/// paragraph is still running, and `tail` lazily continues THE QUOTE.
#[test]
fn a_quote_past_the_column_keeps_the_follower() {
    assert_html(
        ":: term\n:  definition\n    > q\ntail\n",
        "<dl><dt>term</dt><dd><p>definition</p><blockquote><p>q tail</p></blockquote></dd></dl>",
    );
}

/// The AT-column spelling of the same carve-out - the two columns answer alike
/// here too, and the answer is that the follower stays.
#[test]
fn a_quote_at_the_column_keeps_the_follower() {
    assert_html(
        ":: term\n:  definition\n   > q\ntail\n",
        "<dl><dt>term</dt><dd><p>definition</p><blockquote><p>q tail</p></blockquote></dd></dl>",
    );
}

/// Corpus 444-10. Over-indented ORDINARY TEXT opens nothing, the paragraph
/// stays open, and both lines fold into the body.
#[test]
fn over_indented_text_still_folds() {
    assert_html(
        ":: term\n:  definition\n    more\ntail\n",
        "<dl><dt>term</dt><dd>definition more tail</dd></dl>",
    );
}

/// Corpus 444 rows 1 and 2: a link definition past and at the column already
/// ended the body, and still does.
#[test]
fn a_link_definition_ends_the_body_at_both_columns() {
    let expected = "<dl><dt>term</dt><dd>definition</dd></dl><p>tail</p>";
    assert_html(":: term\n:  definition\n    [r]: /url\ntail\n", expected);
    assert_html(":: term\n:  definition\n   [r]: /url\ntail\n", expected);
}

/// Corpus 444-9: a comment past the column. It renders nothing and ends the
/// body, which is where this engine already was.
#[test]
fn a_comment_past_the_column_ends_the_body() {
    assert_html(
        ":: term\n:  definition\n    %% c\ntail\n",
        "<dl><dt>term</dt><dd>definition</dd></dl><p>tail</p>",
    );
}

/// Corpus 430-3, the control the `reached` half of the fix is measured against.
/// BELOW the body's column §10 I5 makes an attribute line lazy paragraph text
/// of THIS container (carve#1809) - the opposite answer to
/// `an_attribute_block_past_the_column_ends_the_body`, on a line the collector
/// hands over with the SAME single residual column.
#[test]
fn an_invisible_line_below_the_column_still_folds() {
    assert_html(
        ":: t\n:  d\n  {.k}\ntail\n",
        "<dl><dt>t</dt><dd>d {.k} tail</dd></dl>",
    );
}

/// Corpus 430-2, the same control for a footnote definition.
#[test]
fn a_footnote_definition_below_the_column_still_folds() {
    assert_html(
        ":: t\n:  d\n  [^f]: n\ntail\n\nSee[^f]\n",
        "<dl><dt>t</dt><dd>d [^f]: n tail</dd></dl><p>See[^f]</p>",
    );
}
