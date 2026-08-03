//! A line block's body is inline content, so a definition-shaped line there is
//! text.
//!
//! `line_block_line = {whitespace}, inline_content, newline` (grammar.ebnf).
//! `reference_definition`, `footnote_definition` and `abbreviation_definition`
//! are BLOCK productions, so none of them can occur inside a line block.
//!
//! Two line-based pre-passes ran before block parsing and did not know what a
//! line block was, so they extracted the definition and blanked the line - it
//! reached neither the definition map's output nor the page. carve-js and
//! carve-php both render these lines (#491).

/// The reference-definition form. The line RENDERS and DEFINES NOTHING.
///
/// This test used to require the opposite of its second half: all three
/// engines registered the definition while rendering the line, so it pinned
/// that pairing and pointed at carve#557 for whether any of them should.
/// carve#574 answered it - nothing inside verse is claimed - and carve-js and
/// carve-php have both stopped registering here, so the assertion flipped.
#[test]
fn a_reference_definition_inside_a_line_block_is_text() {
    let html = carve::to_html("::: |\n[d]: http://x.de\n:::\n\nsee [d][]\n");

    assert!(
        html.contains("[d]: http://x.de"),
        "the line disappeared: {html}"
    );
    assert!(
        !html.contains("href=\"http://x.de\""),
        "a definition inside verse registered: {html}"
    );
}

/// The footnote form. carve-js renders the line as plain text and forms no
/// footnote at all - no reference, no endnote section - and this matches that.
#[test]
fn a_footnote_definition_inside_a_line_block_is_text() {
    let html = carve::to_html("::: |\n[^f]: a note\n:::\n");

    assert!(
        html.contains("[^f]: a note"),
        "the line disappeared: {html}"
    );
    assert!(
        !html.contains("doc-endnotes"),
        "a footnote formed inside a line block: {html}"
    );
}

/// The fence still closes: a line block is not a black hole for the rest of the
/// document, and a definition AFTER it is a real definition again.
#[test]
fn the_line_block_still_ends_and_later_definitions_still_work() {
    let html = carve::to_html("::: |\n[d]: http://in.de\n:::\n\n[e]: http://out.de\n\nsee [e][]\n");

    assert!(html.contains("[d]: http://in.de"), "{html}");
    assert!(
        !html.contains("[e]: http://out.de"),
        "the definition after the line block was not consumed: {html}"
    );
    assert!(html.contains("href=\"http://out.de\""), "{html}");
}

/// An INDENTED opener is not a line block, so the pre-pass must not enter the
/// state on one.
///
/// `detect_line_block_open` trims, so it cannot tell; the block parser can, by
/// the strict column rule for a top-level opener. Entering here on `  ::: |`
/// left every later definition in the document visible and unextracted, which
/// is a worse bug than the one being fixed - it was caught in review, not by a
/// test, which is why this one exists.
#[test]
fn an_indented_opener_does_not_start_a_line_block_in_the_prepass() {
    let html = carve::to_html("  ::: |\n[e]: http://e.de\n\nsee [e][]\n");

    assert!(
        !html.contains("[e]: http://e.de"),
        "the definition after an indented non-opener stayed visible: {html}"
    );
    assert!(html.contains("href=\"http://e.de\""), "{html}");
}

/// A line block opened on a marker line closes at the item's content column,
/// which this line-based pre-pass cannot see - so it must not enter the state
/// there either, or it never leaves it.
///
/// Also caught in review rather than by a test: the state stayed open for the
/// rest of the document and left every later definition visible.
#[test]
fn a_line_block_in_a_list_item_does_not_strand_the_prepass() {
    let html = carve::to_html("- ::: |\n  a\n  :::\n\n[e]: http://e.de\n\nsee [e][]\n");

    assert!(
        !html.contains("[e]: http://e.de"),
        "the definition after a list-item line block stayed visible: {html}"
    );
    assert!(html.contains("href=\"http://e.de\""), "{html}");
}

/// A literal `- :::` inside the verse is TEXT, not the closer.
///
/// The pre-pass used to strip container prefixes before testing the closer, so
/// this line ended the block early and every definition-shaped line between it
/// and the real closer was extracted again. Caught in review.
#[test]
fn a_container_shaped_line_inside_the_verse_is_not_the_closer() {
    let html = carve::to_html("::: |\n- :::\n[d]: http://d.de\n:::\n\nsee [d][]\n");

    assert!(html.contains("- :::"), "{html}");
    assert!(
        html.contains("[d]: http://d.de"),
        "a definition after a literal fence-shaped verse line was lost: {html}"
    );
}

/// Verse text must not move the list-column tracker.
///
/// A body line like `- verse` is not a marker; letting it push a content column
/// left the NEXT top-level opener unprotected, so definitions inside that one
/// disappeared again. Caught in review.
#[test]
fn verse_text_does_not_move_the_list_column_tracker() {
    let html = carve::to_html("::: |\n- verse\n:::\n\n::: |\n[d]: http://d.de\n:::\n");

    assert!(html.contains("- verse"), "{html}");
    assert!(
        html.contains("[d]: http://d.de"),
        "the second line block lost its definition-shaped line: {html}"
    );
}

/// A `%%%` comment fence is opaque, so a literal `::: |` inside one is not an
/// opener.
///
/// #494 entered the pre-pass's line-block state there. The comment's own closer
/// is not a colon fence, so the state stayed open for the rest of the document
/// and every later definition was skipped - a regression that shipped, and was
/// found only when the equivalent carve-php change hit the same hole in review.
#[test]
fn a_line_block_opener_inside_a_comment_fence_is_not_one() {
    let html = carve::to_html("%%%\n::: |\n%%%\n\n[^a]: note\n\nsee [^a]\n");

    assert!(
        !html.contains("[^a]: note"),
        "the definition after the comment stayed visible: {html}"
    );
    assert!(
        html.contains("doc-noteref"),
        "the footnote after the comment stopped resolving: {html}"
    );
}

/// The same for the reference form, which goes through the other pre-pass.
#[test]
fn a_reference_definition_after_a_commented_opener_still_registers() {
    let html = carve::to_html("%%%\n::: |\n%%%\n\n[e]: http://e.de\n\nsee [e][]\n");

    assert!(!html.contains("[e]: http://e.de"), "{html}");
    assert!(html.contains("href=\"http://e.de\""), "{html}");
}

/// An UNTERMINATED `%%%` is not a fenced comment - the block parser degrades it
/// to a single-line comment - so the pre-pass must not treat it as an open
/// fence. Doing so stayed open for the rest of the document and suppressed
/// every later line block, taking the definitions inside them with it.
#[test]
fn an_unterminated_comment_fence_does_not_suppress_later_line_blocks() {
    let html = carve::to_html("%%%\n\n::: |\n[^a]: note\n:::\n\nsee [^a]\n");

    assert!(
        html.contains("[^a]: note"),
        "the verse line was extracted after an unterminated comment: {html}"
    );
    assert!(
        !html.contains("doc-noteref"),
        "a footnote formed from a line block's verse: {html}"
    );
}

/// A comment fence's closer may carry trailing text: `%%% end` closes a `%%%`
/// fence, and the length must match EXACTLY - `%%%%` does not close `%%%`.
///
/// The first version of the closer lookup recorded only bare `%` lines, so a
/// closer like this was invisible and the comment read as unterminated.
#[test]
fn a_comment_closer_may_carry_trailing_text() {
    let html = carve::to_html("%%% trailing\n::: |\n%%% end\n\n[^a]: note\n\nsee [^a]\n");

    assert!(
        html.contains("doc-noteref"),
        "the footnote after a comment closed with trailing text did not resolve: {html}"
    );
}
