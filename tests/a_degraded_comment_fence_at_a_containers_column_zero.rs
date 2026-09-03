//! §28'S DEGRADATION IS A TOTAL CLASSIFICATION, OWNERSHIP INCLUDED
//! (markup-carve/carve#1903, normative in markup-carve/carve#1919).
//!
//! PART 0 and §28 both say a comment-fence opener with no matching closer AHEAD
//! does not open a block and IS one `comment_line`. §24 C3's comment exception
//! then names both spellings and says a comment "does not close the ITEM
//! either", so the ownership half reaches the degraded fence unchanged: at a
//! container's own column 0 it leaves the frame open exactly as `%%` does. A
//! line that were a `comment_line` for rendering and a `comment_block_open` for
//! ownership would be two constructs at once, which no clause offers.
//!
//! WHAT THIS DOES NOT SETTLE, and the two controls that hold it: a TERMINATED
//! fence at that column is a comment BLOCK at the enclosing context's own opener
//! column and DOES end the item (corpus 214), and the closer is looked for AHEAD
//! rather than on the next line, so a fence whose closer sits below a hidden
//! body opens a real block.
//!
//! ORACLE: the executable spec (`scripts/spec/layout.mjs` + `html.mjs`) at carve
//! `35148309`, spec MAIN. The pinned submodule is `95fc3a04`, which predates
//! carve#1919 and still answers the old way, so main is the only revision that
//! can arbitrate this family. Corpus section 445 arrives with the next pin bump.

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
    // Whitespace between tags carries no meaning here and the two spellings
    // indent differently, so it is collapsed on BOTH sides.
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

/// The two spellings of one classification, asked of the same document.
fn assert_substitutes(fence: &str, line: &str) {
    assert_eq!(
        both_paths(fence),
        both_paths(line),
        "the fence spelling and the `%%` line form answer differently"
    );
}

// ---------------------------------------------------------------------------
// THE BAND: an unterminated fence at the frame's own column 0.
// ---------------------------------------------------------------------------

/// Corpus 445-1, the reported document, and 445-2 as its substitution control.
#[test]
fn the_reported_document_keeps_the_follower_in_the_item() {
    assert_html("- x\n%%%\ny\n", "<ul><li>x y</li></ul>");
    assert_substitutes("- x\n%%%\ny\n", "- x\n%% z\ny\n");
}

/// Corpus 445-4: the band is not the bullet's - an ordered marker answers the
/// same at its frame's column 0.
#[test]
fn an_ordered_marker_answers_the_same_way() {
    assert_html("1. x\n%%%\ny\n", "<ol><li>x y</li></ol>");
    assert_substitutes("1. x\n%%%\ny\n", "1. x\n%% z\ny\n");
}

/// Corpus 445-5: the band does not move with the CONTENT column - a padded
/// marker still degrades at column 0.
#[test]
fn a_padded_marker_still_degrades_at_column_zero() {
    assert_html("-   x\n%%%\ny\n", "<ul><li>x y</li></ul>");
    assert_substitutes("-   x\n%%%\ny\n", "-   x\n%% z\ny\n");
}

/// Corpus 445-6: EVERY container's own column 0, at every depth. The inner
/// item's is 2, and it keeps its own follower.
#[test]
fn a_nested_frames_own_column_zero_answers_too() {
    assert_html(
        "- - x\n  %%%\n  y\n",
        "<ul><li><ul><li>x y</li></ul></li></ul>",
    );
    assert_substitutes("- - x\n  %%%\n  y\n", "- - x\n  %% z\n  y\n");
}

/// Corpus 445-7: a wider unterminated run degrades the same - length is not
/// what decides it.
#[test]
fn a_wider_unterminated_run_degrades_too() {
    assert_html("- x\n%%%%\ny\n", "<ul><li>x y</li></ul>");
    assert_substitutes("- x\n%%%%\ny\n", "- x\n%% z\ny\n");
}

/// Corpus 445-8: §28 matches a closer on EXACT length, so a WIDER run below the
/// opener is not one and the opener stays degraded.
#[test]
fn a_wider_run_below_the_opener_is_not_a_closer() {
    assert_html("- x\n%%%\n%%%%\ny\n", "<ul><li>x y</li></ul>");
}

/// Corpus 445-10: the degraded comment leaves the LIST open, not only the item,
/// so the sibling marker below it is the same list's second item.
#[test]
fn the_list_stays_open_for_a_sibling_marker() {
    assert_html("- x\n%%%\n- y\n", "<ul><li>x</li><li>y</li></ul>");
    assert_substitutes("- x\n%%%\n- y\n", "- x\n%% z\n- y\n");
}

/// The `- a` / `  - x` shape, where the fence at the DOCUMENT's column 0 sits
/// below every live content column: it reached no container, so the innermost
/// open item keeps the line under it. This is the row `a_below_column_comment_
/// ends_no_item` used to pin the other way.
#[test]
fn a_column_zero_fence_below_every_content_column_keeps_the_inner_item() {
    assert_html(
        "- a\n  - x\n%%% c\ntail\n",
        "<ul><li>a<ul><li>x tail</li></ul></li></ul>",
    );
    assert_substitutes("- a\n  - x\n%%% c\ntail\n", "- a\n  - x\n%% c\ntail\n");
}

/// A MARKER LADDER (`- - x`) re-parses a dedented copy of the same chunk at
/// every level, and the frame that owns a fence at the DOCUMENT's column 0 is
/// the innermost one - the fence reached no container, so the innermost open
/// item keeps the line under it. Only the outer frame's own loop sees the fence
/// as flush-left; the ladder's arm is a second, separate site.
#[test]
fn a_marker_ladder_keeps_the_follower_in_the_innermost_item() {
    assert_html("- - x\n%%%\ny\n", "<ul><li><ul><li>x y</li></ul></li></ul>");
    assert_substitutes("- - x\n%%%\ny\n", "- - x\n%% z\ny\n");
}

/// The same one rung deeper, so a fix that reaches exactly one level short of
/// the innermost frame fails here rather than passing by accident.
#[test]
fn a_three_rung_ladder_answers_the_same_way() {
    assert_html(
        "- - - x\n%%%\ny\n",
        "<ul><li><ul><li><ul><li>x y</li></ul></li></ul></li></ul>",
    );
    assert_substitutes("- - - x\n%%%\ny\n", "- - - x\n%% z\ny\n");
}

/// A BLANK-SEPARATED sub-list reaches the fold arm rather than the ladder's,
/// and answers the same.
#[test]
fn a_blank_separated_sublist_keeps_the_follower_too() {
    assert_html(
        "- a\n\n  - x\n%%% c\ntail\n",
        "<ul><li>a<ul><li>x tail</li></ul></li></ul>",
    );
    assert_substitutes("- a\n\n  - x\n%%% c\ntail\n", "- a\n\n  - x\n%% c\ntail\n");
}

// ---------------------------------------------------------------------------
// THE CONTROLS. Each of these fails a fix that overshoots.
// ---------------------------------------------------------------------------

/// Corpus 445-2: the `%%` line form, the answer every reader already gives and
/// the one the substitution requires the fence spelling to equal.
#[test]
fn the_line_form_is_unchanged() {
    assert_html("- x\n%% z\ny\n", "<ul><li>x y</li></ul>");
}

/// Corpus 445-3: a TERMINATED fence at that column is a comment BLOCK written
/// at the document's own opener column and it DOES end the item (corpus 214). A
/// fix that degrades every fence rather than only the unterminated one breaks
/// this.
#[test]
fn a_terminated_fence_at_column_zero_still_ends_the_item() {
    assert_html("- x\n%%%\n%%%\ny\n", "<ul><li>x</li></ul><p>y</p>");
}

/// Corpus 445-9: the closer is looked for AHEAD, not on the next line. This
/// fence opens a real block, hides `y`, and ends the item at the document's own
/// opener column. A fix that degrades on "no closer immediately below" breaks
/// this.
#[test]
fn the_closer_is_looked_for_ahead_not_on_the_next_line() {
    assert_html("- x\n%%%\ny\n%%%\nz\n", "<ul><li>x</li></ul><p>z</p>");
}

/// Corpus 445-11: the exception is §24 C3's, about ITEM ownership. A block
/// quote has no such clause, and a comment at column 0 below it ends the quote
/// in BOTH spellings. A fix that read the substitution as "a degraded fence
/// never closes anything" breaks this.
#[test]
fn a_quote_ends_in_both_spellings() {
    assert_html("> x\n%%%\ny\n", "<blockquote><p>x</p></blockquote><p>y</p>");
    assert_substitutes("> x\n%%%\ny\n", "> x\n%% z\ny\n");
}

/// The terminated control for
/// `a_column_zero_fence_below_every_content_column_keeps_the_inner_item`: with
/// a closer ahead the same line is a comment BLOCK and the item ends.
#[test]
fn a_terminated_fence_below_every_content_column_still_ends_the_item() {
    let html = both_paths("- a\n  - x\n%%% c\n%%%\ntail\n");
    assert!(html.ends_with("<p>tail</p>"), "{html}");
}

/// The terminated control for the ladder rows: with a closer ahead the fence is
/// a comment BLOCK at the document's own opener column and the ladder ends.
#[test]
fn a_terminated_fence_under_a_ladder_still_ends_the_item() {
    assert_html(
        "- - x\n%%%\n%%%\ny\n",
        "<ul><li><ul><li>x</li></ul></li></ul><p>y</p>",
    );
}

/// A fence at the item's own CONTENT column is a different band and keeps its
/// answer: corpus 443 pins it, and this ruling did not touch it.
#[test]
fn a_fence_at_the_content_column_is_a_different_band() {
    assert_html("- x\n  %%%\ny\n", "<ul><li>x</li></ul><p>y</p>");
    assert_html("- x\n  %%%\n  y\n", "<ul><li>x y</li></ul>");
}
