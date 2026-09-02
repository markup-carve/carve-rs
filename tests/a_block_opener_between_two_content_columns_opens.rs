//! A BLOCK OPENER written between two live content columns belongs to the outer
//! of the two and opens there (markup-carve/carve-rs#1506, ruled by
//! markup-carve/carve#1896).
//!
//! The block-layer twin of carve-rs#1505. That one corrected the definition
//! PREPASS; the same question asked of block parsing kept the old answer, so a
//! heading, a thematic break, a fence, a quote, a table, a div, a definition
//! list or an attribute line written at the same indent still folded into the
//! open paragraph as text.
//!
//! The reported document is `- - x` with `   # H` under it. The live columns
//! are 2 and 4, the opener sits at 3, and the item at 4 opened on the marker
//! line above and never on this one - so it owns nothing here and the item at 2
//! does. The title of the ticket says "fence"; the case is every opener.
//!
//! MEASURED, NOT ASSUMED. 306 prefix/column pairs over the l(ist)/q(uote)
//! container prefixes to depth four, the line written at every column from just
//! past the last quote marker out to 14, sixteen line kinds each, run through
//! the executable spec at the pinned corpus (`tests/spec` at carve `86569bd`)
//! and through this engine. Before: 11 pairs disagreed for each of the heading,
//! thematic-break, fence, quote, table, div, definition-list and
//! attribute-block kinds, always the engine folding what the spec opens. After:
//! 0 for every one of them, with the list-marker, ordered-marker, paragraph,
//! comment and definition kinds unmoved at 0 throughout. Every expectation
//! below is the executable spec's own output for that document.

use carve::to_html;

/// The eleven (marker line, opener line) pairs the sweep found - the same
/// eleven the definition prepass had in carve-rs#1505, and every one of them a
/// column strictly between two live LIST content columns.
const BETWEEN: &[(&str, &str)] = &[
    ("- - x", "   # H"),
    ("- - - x", "   # H"),
    ("- - - x", "     # H"),
    ("> - - x", ">    # H"),
    ("- - - - x", "   # H"),
    ("- - - - x", "     # H"),
    ("- - - - x", "       # H"),
    ("- > - - x", "  >    # H"),
    ("> - - - x", ">    # H"),
    ("> - - - x", ">      # H"),
    ("> > - - x", "> >    # H"),
];

#[test]
fn the_reported_document_renders_what_the_spec_renders() {
    let html = to_html("- - x\n   # H\n");
    assert_eq!(
        html.trim(),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <ul>\n",
            "      <li>x</li>\n",
            "    </ul>\n",
            "    <h1 id=\"H\">H</h1>\n",
            "  </li>\n",
            "</ul>",
        ),
        "{html}"
    );
}

#[test]
fn the_ticket_s_own_fence_opens_in_the_outer_item() {
    // The shape carve-rs#1506 was filed for. It rendered as inline code inside
    // the inner item because the fence folded as a lazy continuation.
    let html = to_html("- - x\n   ```\n   [r]: /url\n   ```\n");
    assert_eq!(
        html.trim(),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <ul>\n",
            "      <li>x</li>\n",
            "    </ul>\n",
            "    <pre><code>[r]: /url\n",
            "</code></pre>\n",
            "  </li>\n",
            "</ul>",
        ),
        "{html}"
    );
}

#[test]
fn every_opener_kind_answers_the_same_way() {
    // One rule over block openers, not one rule per spelling. Each expectation
    // is the executable spec's output for that document.
    for (opener, expected) in [
        ("   ---", "<hr>"),
        ("   > q", "<blockquote><p>q</p></blockquote>"),
        ("   ::: note\n   b\n   :::", "aria-label=\"Note\""),
        ("   :: t\n   : d", "<dd>d</dd>"),
        ("   {.c}\n   b", "<p class=\"c\">b</p>"),
        ("   | A |\n   | - |\n   | b |", "<td>b</td>"),
    ] {
        let src = format!("- - x\n{opener}\n");
        let html = to_html(&src);
        assert!(html.contains(expected), "{src:?}: {html}");
        // The inner item closes before it, so its paragraph holds `x` alone.
        assert!(html.contains("<li>x</li>"), "{src:?}: {html}");
    }
}

#[test]
fn every_between_column_pair_opens_the_block() {
    for (marker, opener) in BETWEEN {
        let src = format!("{marker}\n{opener}\n");
        let html = to_html(&src);
        assert!(html.contains("<h1 id=\"H\">H</h1>"), "{src:?}: {html}");
        assert!(
            !html.contains("x\n# H"),
            "the opener folded into the paragraph: {src:?}: {html}"
        );
    }
}

#[test]
fn all_four_neighbouring_columns_answer_the_same_way() {
    // What markup-carve/carve#1896 asks a corpus row to pin, kept here because
    // the corpus lives in the pinned `tests/spec` submodule and is not authored
    // in this repo. The hole was column 3 alone: 2 opened in the outer item and
    // 4 in the inner one on either side of it. Column 1 reaches nothing and is
    // the floor below every live column.
    let below = to_html("- - x\n # H\n");
    assert!(
        below.contains("x\n# H"),
        "column 1 stopped folding: {below}"
    );
    assert!(!below.contains("<h1"), "column 1 opened a heading: {below}");

    let outer = to_html("- - x\n  # H\n");
    assert!(outer.contains("<h1 id=\"H\">H</h1>"), "column 2: {outer}");
    assert_eq!(
        to_html("- - x\n   # H\n"),
        outer,
        "column 3 does not answer as column 2 does"
    );

    let inner = to_html("- - x\n    # H\n");
    assert_ne!(inner, outer, "column 4 answers as column 2 does");
    assert_eq!(
        to_html("- - x\n     # H\n"),
        inner,
        "column 5 does not answer as column 4 does"
    );
}

#[test]
fn a_wider_marker_carries_the_columns_with_it() {
    // `1. ` opens content at column 3, so `1. 1. x` has live columns 3 and 6 and
    // the band between them is TWO columns wide. Both belong to the outer item,
    // which is what makes this a question about the column a line reaches
    // rather than about one residual space.
    let outer = to_html("1. 1. x\n   # H\n");
    assert!(outer.contains("<h1 id=\"H\">H</h1>"), "{outer}");
    for indent in [4usize, 5] {
        let html = to_html(&format!("1. 1. x\n{}# H\n", " ".repeat(indent)));
        assert_eq!(
            html, outer,
            "column {indent} does not answer as column 3 does: {html}"
        );
    }
}

#[test]
fn the_sublist_may_open_on_a_following_line_too() {
    // Nothing here is about the MARKER line: the same two columns are live when
    // the inner list opens on its own line, with or without a blank above it.
    for src in [
        "- x\n  - y\n   # H\n",
        "- x\n\n  - y\n   # H\n",
        "- x\n  - y\n\n   # H\n",
    ] {
        let html = to_html(src);
        assert!(html.contains("<h1 id=\"H\">H</h1>"), "{src:?}: {html}");
        assert!(html.contains("<li>y</li>"), "{src:?}: {html}");
    }
}

#[test]
fn a_list_marker_between_two_content_columns_still_folds() {
    // MEASURED SURVIVOR, not an oversight. A marker written in the same band
    // folds as lazy item text in the executable spec, where every other opener
    // shape opens - the sweep puts the marker and ordered-marker kinds at 0
    // divergences before and after. A fix that widened the rule to markers
    // would take this with it.
    for opener in ["   - y", "   1. y"] {
        let src = format!("- - x\n{opener}\n");
        let html = to_html(&src);
        assert!(
            html.contains(opener.trim_start()),
            "the marker stopped being text: {src:?}: {html}"
        );
        assert!(!html.contains("<li>y"), "{src:?}: {html}");
    }
}

#[test]
fn an_over_indented_definition_description_still_folds() {
    // THE CONTROL A COLUMN-COLLAPSING FIX BREAKS, and corpus row
    // `154-under-indented-definition-attaches-over-indented-definition-folds-3`
    // pins it: the term sits AT the item's column and the description one past
    // it, so the description is over-indented RELATIVE TO ITS TERM and folds
    // into the `<dt>`. Absorbing the residual column outright renders a `<dd>`
    // here.
    let html = to_html("- one\n  :: term\n   :  def\n");
    assert!(html.contains("<dt>term\n :  def</dt>"), "{html}");
    assert!(!html.contains("<dd>"), "{html}");
}

#[test]
fn a_line_that_reaches_no_column_is_still_lazy_paragraph_text() {
    // THE CONTROLS. Absorbing the residual indent is allowed only once the line
    // has REACHED a content column; below every column there is nothing to
    // absorb it into and the line continues the open paragraph. Each renders
    // byte-identically to the executable spec.
    for (src, folded) in [
        ("- x\n # H\n", "x\n# H"),
        ("- - x\n # H\n", "x\n# H"),
        ("- > - - x\n  >  # H\n", "x\n# H"),
        // A folded thematic break is text, and text gets the typographic dash.
        ("- - x\n ---\n", "x\n\u{2014}"),
    ] {
        let html = to_html(src);
        assert!(html.contains(folded), "{src:?}: {html}");
        assert!(
            !html.contains("<h1") && !html.contains("<hr>"),
            "a below-column line opened a block: {src:?}: {html}"
        );
    }
}
