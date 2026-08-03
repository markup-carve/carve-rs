//! `carve fmt` must not turn a comment's contents into visible text.
//!
//! `restore_inline_comments` walks the SOURCE lines looking for a trailing `%%`
//! comment to graft back onto the formatted output. It treated a comment-BLOCK
//! fence as one: inside a blockquote, `> %%%` split into `>` plus `%%%`, and
//! since `>` renders as the blank quoted line, the fence text was appended
//! there. That unbalanced the real fence, so the commented-out body came back
//! as a paragraph (carve-rs#432).
//!
//! Only the top-level case escaped, and by accident - there the part before the
//! fence is empty, so a `before.trim()` guard skipped it. That is why the bug
//! needed a blockquote to show up, and why the corpus documents that contain it
//! still passed: nothing re-rendered the formatter's output.
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

/// The other direction: a real trailing comment must still be restored, which
/// is what `restore_inline_comments` exists for. A fix that simply stopped
/// matching `%` would pass every test above and silently drop these.
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
