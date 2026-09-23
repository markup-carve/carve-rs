//! markup-carve/carve-rs#1831. PART 11 section 1a (`CARVE-P11-002`) permits only
//! the smallest departure that restores an invariant and is not a license to
//! respell, so the writer may only fall back to inline HTML where the ordinary
//! `**`/`*` spelling does not read back.
//!
//! An escaped marker is TEXT, so a part ending in `\*` contributes a live run
//! of length 0 (markup-carve/carve-rs#1654) and nothing merges across the seam.
//! The seam pass asked about the merged run anyway, and the flanking test it
//! asks fails on the backslash, so the emphasis after such a part became a tag.

use carve::{markdown_to_ast, parse, render_html, render_markdown, to_markdown};

fn md(src: &str) -> String {
    render_markdown(&parse(src)).expect("the document writes")
}

/// What a CommonMark reader makes of the Markdown this renderer wrote, against
/// what the document itself renders to. The reader is pulldown-cmark, so this
/// is the ecosystem's own reading rather than a second opinion from the engine
/// that produced the bytes.
///
/// This is what licenses dropping the fallback, not what pins the defect: raw
/// HTML passes through a reader unchanged, so the tag form reads back too. The
/// exact-Markdown assertions above are the regression pin.
fn reads_back(src: &str) -> (String, String) {
    let back = render_html(&markdown_to_ast(&to_markdown(src))).expect("the reader renders");

    (
        back,
        render_html(&parse(src)).expect("the document renders"),
    )
}

// ---------------------------------------------------------------------------
// The defect.
// ---------------------------------------------------------------------------

#[test]
fn a_strong_after_an_escaped_marker_keeps_its_delimiters() {
    // Corpus `484-a-delimiter-after-an-underscore-or-slash-opens-only-when-that
    // -one-pairs-4`, the one Markdown row the cross-engine conformance run
    // reported as a difference.
    assert_eq!(md("_*{*x*} q"), "_\\***x** q\n");
}

#[test]
fn an_italic_after_an_escaped_marker_keeps_its_delimiters() {
    // Not in the corpus, same seam.
    assert_eq!(md("_*{/x/} q"), "_\\**x* q\n");
}

#[test]
fn the_delimiter_spelling_reads_back_to_the_documents_own_html() {
    for src in ["_*{*x*} q\n", "_*{/x/} q\n"] {
        let (back, own) = reads_back(src);
        assert_eq!(back, own, "{src}");
    }
}

// ---------------------------------------------------------------------------
// CONTROLS. Each of these agrees across carve-rs, carve-js and carve-php today
// and must keep doing so.
// ---------------------------------------------------------------------------

#[test]
fn a_space_in_front_of_the_emphasis_is_untouched() {
    assert_eq!(md("a\\* {*x*} q"), "a\\* **x** q\n");
}

#[test]
fn a_strike_across_the_same_seam_is_untouched() {
    assert_eq!(md("_*{~x~} q"), "_\\*~~x~~ q\n");
}

#[test]
fn a_live_trailing_run_of_one_still_unmerges() {
    // The left part is `*x\**`: the escaped marker is text, but the delimiter
    // behind it is live, so the run is 1 and the seam is a real one.
    assert_eq!(md("{/x*/}{/~y/}"), "*x\\**<em>\\~y</em>\n");
}

#[test]
fn the_controls_read_back_to_their_own_html() {
    for src in ["a\\* {*x*} q\n", "_*{~x~} q\n", "{/x*/}{/~y/}\n"] {
        let (back, own) = reads_back(src);
        assert_eq!(back, own, "{src}");
    }
}
