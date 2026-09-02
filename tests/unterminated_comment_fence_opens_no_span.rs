//! An unterminated comment fence opens no span, and it ends the item it was
//! written in.
//!
//! §28: a fence with no closer degrades to an ordinary `%%` line comment. The
//! item collector opened a span for one anyway, and the span's dedent then
//! lifted a BELOW-column line to the body's column 0, where it parsed as a
//! block (carve-rs#586). The quoted-definition cases below came with the same
//! sweep and are pinned here beside it - they landed in #589.
//!
//! WHAT #586 DID NOT MEASURE was where the line goes once it is not lifted. The
//! degraded fence closes the item's paragraph, so a line written BELOW the
//! item's content column continues nothing and the list ends - the line
//! reparses at document level (markup-carve/carve-rs#1512). This file asserted
//! the folded reading for four years' worth of engine behavior and was never
//! compared against the executable spec; it is now, at the pinned corpus
//! (`tests/spec` at carve `86569bd`).

use carve::to_html;

// --- An unterminated comment fence opens no span (carve-rs#586) ---

#[test]
fn an_unterminated_fence_ends_the_item_and_does_not_lift_the_line() {
    // §28: a fence with no closer degrades to a `%%` line comment, so the lines
    // after it are just lines - and the comment closed the item's paragraph, so
    // a line below the item's content column continues nothing and the list
    // ends. The line reparses at document level, where its own column keeps it
    // paragraph text.
    //
    // BOTH HALVES MATTER. #586's defect was the span's dedent LIFTING the line
    // to column 0, where it parsed as a heading; that is still guarded by the
    // `<h1>` assertion below. What #586 left unmeasured was where the
    // unlifted line goes, and the folded reading it settled on is not the
    // executable spec's.
    let html = to_html("- a\n  %%% x\n # h");
    assert_eq!(html, "<ul>\n  <li>a</li>\n</ul>\n<p># h</p>", "{html}");
    assert!(
        !html.contains("<h1"),
        "the line was lifted to column 0: {html}"
    );
}

#[test]
fn a_line_comment_leaves_the_item_open_where_a_degraded_fence_ends_it() {
    // THE CONTROL, and the reason the rule is about the FENCE rather than about
    // comments. A `%%` line comment written at the same column closes the
    // paragraph too, but the item survives it and the below-column line stays
    // inside - the executable spec's own answer for that document, measured
    // rather than inherited.
    let html = to_html("- a\n  %% x\n b");
    assert_eq!(html, "<ul>\n  <li>a\n    b\n  </li>\n</ul>", "{html}");
}

#[test]
fn a_fence_below_the_item_s_column_ends_nothing() {
    // The other control: a fence the item never saw. Written BELOW the content
    // column it reaches no container, so it is lazy text of the item's
    // paragraph and the line under it stays where it was.
    let html = to_html("- a\n %%% x\n b");
    assert_eq!(html, "<ul>\n  <li>a\n    b\n  </li>\n</ul>", "{html}");
}

#[test]
fn a_terminated_fence_leaves_the_item_open() {
    // A fence with a closer IS a fence: it opens a span, hides its body, and
    // ends nothing. Only the degraded spelling closes the item.
    let html = to_html("- x\n  %%% c\n  %%%\n b");
    assert_eq!(html, "<ul>\n  <li>x\n    b\n  </li>\n</ul>", "{html}");
}

#[test]
fn one_level_deeper_the_line_is_between_two_columns_instead() {
    // NOT the same shape, which is why it does not answer the same way. Under
    // `- - a` the live columns are 2 and 4, so `   # h` is written BETWEEN them
    // rather than below the only column there is: it reaches the outer item and
    // opens there (markup-carve/carve-rs#1506, ruled by markup-carve/carve#1896).
    //
    // This test asserted the folded reading. The executable spec never had it -
    // measured against it here for the first time, at the pinned corpus.
    let expected =
        "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>";
    assert_eq!(to_html("- - a\n    %%% x\n   # h"), expected);
    // The degraded fence is beside the point: the same two columns answer the
    // same way with nothing between them.
    assert_eq!(to_html("- - a\n   # h"), expected);
}

#[test]
fn a_terminated_fence_still_travels_as_one_span() {
    // The control: with a closer it IS a fence, its body is hidden, and the
    // span keeps its own columns.
    assert_eq!(
        to_html("- - a\n %%% c\n x\n %%%\n b"),
        "<ul>\n  <li>\n    <ul>\n      <li>a\n        b\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

// --- A definition inside a quote inside an item (carve-rs#588) ---

#[test]
fn a_quoted_definition_in_a_list_item_registers() {
    // `strip_blockquote_prefix` reads a flush-left `>` only, so a quote written
    // INSIDE an item arrived after the content column and the definition never
    // registered - while the same line one level up is collected and empties
    // the quote.
    assert_eq!(
        to_html("- a\n  > [r]: /u\n\nsee [t][r]"),
        "<ul>\n  <li>a\n    <blockquote>\n\n    </blockquote>\n  </li>\n</ul>\n<p>see <a href=\"/u\">t</a></p>"
    );
}

#[test]
fn the_footnote_form_registers_too() {
    let html = to_html("- a\n  > [^f]: x\n\nsee[^f]");

    assert!(html.contains("doc-noteref"), "{html}");
    assert!(
        html.contains("<blockquote>"),
        "the emptied quote stays in the item: {html}"
    );
}

#[test]
fn the_top_level_shape_is_unchanged() {
    assert_eq!(
        to_html("> [r]: /u\n\nsee [t][r]"),
        "<blockquote>\n\n</blockquote>\n<p>see <a href=\"/u\">t</a></p>"
    );
}
