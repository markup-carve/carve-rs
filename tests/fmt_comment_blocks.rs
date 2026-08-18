//! `carve fmt` must not turn a comment's contents into visible text.
//!
//! `to_carve` used to walk the SOURCE lines looking for a trailing `%%` comment
//! to graft back onto the formatted output. It treated a comment-BLOCK fence as
//! one: inside a blockquote, `> %%%` split into `>` plus `%%%`, and since `>`
//! renders as the blank quoted line, the fence text was appended there. That
//! unbalanced the real fence, so the commented-out body came back as a paragraph
//! (carve-rs#432).
//!
//! Only the top-level case escaped, and by accident - there the part before the
//! fence is empty, so a `before.trim()` guard skipped it. That is why the bug
//! needed a blockquote to show up, and why the corpus documents that contain it
//! still passed: nothing re-rendered the formatter's output.
//!
//! The re-graft is GONE (carve-rs#1076): the writer emits the `comment` node
//! itself, so the helper could only ever append to a line that did not already
//! carry the marker. These stay as the guard that removing it did not take a real
//! comment with it.
//!
//! These assert the PROPERTY - `to_html(fmt(x)) == to_html(x)` - rather than the
//! emitted bytes. The spelling of the output is allowed to differ from the
//! input; what the document says is not.

fn preserves(source: &str) {
    let formatted = carve::to_carve(source);
    assert_eq!(
        carve::to_html(&formatted),
        carve::to_html(source),
        "formatting changed the document\n--- source ---\n{source}\n--- formatted ---\n{formatted}",
    );
}

#[test]
fn a_comment_after_a_paragraph_in_a_blockquote_stays_a_comment() {
    preserves("> a\n>\n> %%%\n> x\n> %%%\n");
}

#[test]
fn a_comment_before_a_paragraph_in_a_blockquote_stays_a_comment() {
    preserves("> %%%\n> x\n> %%%\n>\n> b\n");
}

#[test]
fn a_comment_alone_in_a_blockquote_is_unchanged() {
    preserves("> %%%\n> x\n> %%%\n");
}

#[test]
fn a_top_level_comment_beside_a_paragraph_is_unchanged() {
    preserves("a\n\n%%%\nx\n%%%\n");
}

/// The other direction: a real trailing comment must still come out. Every test
/// above passes on a formatter that drops trailing comments entirely, so these
/// are what stop that reading of "stop matching `%`".
#[test]
fn a_trailing_inline_comment_survives() {
    let formatted = carve::to_carve("text %% a note\n\nmore\n");
    assert!(
        formatted.contains("%% a note"),
        "the inline comment was dropped: {formatted:?}",
    );
}

#[test]
fn a_trailing_inline_comment_inside_a_blockquote_survives() {
    let formatted = carve::to_carve("> text %% a note\n");
    assert!(
        formatted.contains("%% a note"),
        "the quoted inline comment was dropped: {formatted:?}",
    );
}

/// The re-graft wrote a trailing comment TWICE and put the copy on an EARLIER
/// line: it matched the first formatted line equal to the part before the marker,
/// and `a` above `a %%` is that line. Both properties this file rests on survived
/// it - a comment renders nothing, and the writer was idempotent about its own
/// spelling - so the TREE is what shows it (carve-rs#1076).
#[test]
fn a_repeated_line_above_a_trailing_comment_gets_no_copy_of_it() {
    let source = "::: |\na\na %%\n:::\n";
    let formatted = carve::to_carve(source);
    assert_eq!(
        formatted, source,
        "the writer changed a document that is already canonical",
    );
    assert_eq!(
        carve::parse(&formatted).children,
        carve::parse(source).children,
        "formatting changed the tree\n--- source ---\n{source}\n--- formatted ---\n{formatted}",
    );
}

/// Not line-block-specific: the same double-write was reachable in a paragraph
/// wherever the part before the marker repeats as a whole earlier line.
#[test]
fn a_repeated_paragraph_line_above_a_trailing_comment_gets_no_copy_of_it() {
    let source = "a\n\na %% note\n";
    let formatted = carve::to_carve(source);
    assert_eq!(
        formatted.matches("%%").count(),
        1,
        "the comment was written more than once: {formatted:?}",
    );
    assert_eq!(
        carve::parse(&formatted).children,
        carve::parse(source).children,
        "formatting changed the tree\n--- source ---\n{source}\n--- formatted ---\n{formatted}",
    );
}
