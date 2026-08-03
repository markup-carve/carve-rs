//! A lazy continuation line continues the innermost OPEN PARAGRAPH, however
//! many containers it failed to match.
//!
//! This engine applied that at depth 1 and not at depth 2: `> a` / `b` folded,
//! `> a` / `>> b` / `c` closed both quotes and started a sibling paragraph.
//! carve-js and carve-php fold at both depths (carve-rs#452).
//!
//! The depth-1 behavior is what settles it. Read strictly, PART 1 S4 ("the
//! innermost MATCHED context holds an OPEN PARAGRAPH") would close the quote at
//! depth 1 too, since a bare line matches zero containers there as well - and
//! no engine does that, corpus 80-blockquote-lazy-continuation pins it. So the
//! strict reading is not what anyone implements, and being inconsistent between
//! depth 1 and depth 2 was not a reading of it (markup-carve/carve#506).

#[test]
fn a_lazy_line_continues_a_nested_quote() {
    assert_eq!(
        carve::to_html("> a\n>> b\nc\n"),
        "<blockquote>\n  <p>a</p>\n  <blockquote><p>b\nc</p></blockquote>\n</blockquote>"
    );
}

#[test]
fn the_spaced_spelling_behaves_the_same() {
    // `> > b` and `>> b` are the same document; the divergence was about depth,
    // not about the marker spelling.
    assert_eq!(
        carve::to_html("> a\n> > b\nc\n"),
        carve::to_html("> a\n>> b\nc\n")
    );
}

#[test]
fn depth_one_is_unchanged() {
    // The behavior this fix generalises from. Pinned so a change to the nested
    // path cannot quietly take the simple case with it.
    assert_eq!(
        carve::to_html("> a\nb\n"),
        "<blockquote><p>a\nb</p></blockquote>"
    );
}

#[test]
fn three_levels_fold_into_the_innermost() {
    let html = carve::to_html("> a\n>> b\n>>> c\nd\n");
    assert!(
        html.contains("c\nd"),
        "the lazy line should join the innermost paragraph, got: {html}"
    );
}

#[test]
fn a_block_opener_still_ends_the_quote() {
    // Laziness folds PLAIN text. A visible opener after a nested quote closes
    // it and starts that block outside, exactly as at depth 1.
    let html = carve::to_html("> a\n>> b\n# H\n");
    assert!(
        html.contains("</blockquote>") && html.contains("<h1"),
        "a heading must end the quote, got: {html}"
    );
    assert!(
        !html.contains("b\n# H"),
        "the heading must not fold into the quoted paragraph, got: {html}"
    );
}

#[test]
fn a_blank_line_still_ends_the_quote() {
    let html = carve::to_html("> a\n>> b\n\nc\n");
    assert!(
        html.contains("<p>c</p>") && !html.contains("b\nc"),
        "a blank line ends the quote, got: {html}"
    );
}

#[test]
fn a_nested_container_opener_still_closes_the_paragraph() {
    // `> ::: note` is a nested quote whose CONTENT is a container opener, so
    // the paragraph does not stay open and the following bare line does not
    // fold. The fix looks through blockquote markers, not through everything.
    let html = carve::to_html("> a\n> ::: note\n> body\n> :::\nc\n");
    assert!(
        !html.contains("body\nc"),
        "an admonition body must not absorb the lazy line, got: {html}"
    );
}
