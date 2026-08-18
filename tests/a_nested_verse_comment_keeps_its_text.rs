//! A verse comment keeps its text wherever its boundary ended up
//! (markup-carve/carve-rs#1079).
//!
//! A comment-only body line in a line block is emptied at the block layer
//! (markup-carve/carve#1333) and put back into the tree as the `comment` node it
//! is, at the boundary that ends its line, so `carve fmt` writes the author's
//! own line back. That pass counted the stanza's TOP-LEVEL `hard_break` nodes,
//! which is ONE of the three spellings a boundary has, and each of the two it
//! missed lost a comment:
//!
//! - An inline container that opens on one body line and closes on a later one
//!   holds the boundaries between them as its OWN children, so the walk never
//!   saw them and the node found nowhere to sit.
//! - A CLOSED verbatim run spanning a boundary carries that newline in its
//!   VALUE rather than as a node, so counting nodes alone reported a line
//!   number short and every comment after it was placed a line late.
//!
//! NEITHER GATE COULD SEE EITHER. The comment publishes nothing, so the HTML is
//! identical before and after; and the empty line the writer emitted re-parses
//! to the same (lossy) tree the loss produced, so `parse(fmt(x)) == parse(x)`
//! held while the text was gone - the limit named on markup-carve/carve#1340.
//! The assertions here are therefore on the TREE and on the written BYTES.
//!
//! The break SPELLING at a nested boundary is a separate and contested question
//! (markup-carve/carve#1351) and nothing here depends on it: this pass asks
//! where the boundary IS, never what it is called. The soft-to-hard conversion
//! is untouched and still runs at the stanza's top level only, pinned below.

use carve::to_html;
use carve::{parse, render_carve};

fn fmt(src: &str) -> String {
    render_carve(&parse(src)).expect("a line block writes back")
}

fn tree(src: &str) -> String {
    format!("{:?}", parse(src))
}

/// Every shape here is a document the author wrote; `fmt` must write it back.
fn assert_round_trips(src: &str) {
    assert_eq!(fmt(src), src, "fmt did not write the document back");
    assert_eq!(fmt(&fmt(src)), fmt(src), "fmt is not idempotent here");
}

#[test]
fn a_comment_under_an_emphasis_span_keeps_its_text() {
    // The reported shape. The `*` opens on the first body line and closes on the
    // last, so the boundary that ends the comment's line is a child of the
    // `strong` and the top-level walk never reached it.
    let src = "::: |\n*a\n%% secret\nc*\n:::\n";
    assert!(
        tree(src).contains("secret"),
        "the note's text is gone from the tree: {}",
        tree(src)
    );
    assert_round_trips(src);
}

#[test]
fn the_writer_no_longer_emits_an_empty_line() {
    // The half that was this engine's alone. With the node dropped there was
    // nothing to write, so the line came back EMPTY - and a blank line ENDS a
    // stanza, so that was a different document. carve-js and carve-php wrote a
    // bare `%%` here, which at least re-parsed to the same tree.
    //
    // Both halves fall out of the one fix: the writer emits the author's line
    // because the node carrying it is back.
    let written = fmt("::: |\n*a\n%% secret\nc*\n:::\n");
    assert!(
        !written.contains("\n\n"),
        "wrote a blank line, which ends the stanza: {written:?}"
    );
    assert!(written.contains("%% secret"), "{written:?}");
}

#[test]
fn every_slot_an_inline_node_holds_inlines_in() {
    // Not just `children`. An inline footnote carries its body in `inline` and a
    // citation item carries three arrays of its own, and a walk that knew only
    // `children` missed both - the same two slots
    // markup-carve/carve-js#1184 found after its first pass.
    for src in [
        "::: |\n*a\n%% secret\nc*\n:::\n",         // emphasis
        "::: |\n[a\n%% secret\nc](/u)\n:::\n",     // link
        "::: |\n{.x}a\n%% secret\nc{.x}\n:::\n",   // span
        "::: |\n{+a\n%% secret\nc+}\n:::\n",       // inline extension
        "::: |\n^[a\n%% secret\nc]\n:::\n",        // footnote body
        "::: |\n[see @a\n%% secret\np. 1]\n:::\n", // citation prefix/suffix
    ] {
        assert!(
            tree(src).contains("secret"),
            "the note's text is gone for {src:?}"
        );
        assert_round_trips(src);
    }
}

#[test]
fn a_closed_run_that_ate_a_boundary_does_not_shift_the_rest() {
    // The second missed spelling, and the one no ticket reported. The code span
    // opens on body line 0 and closes on line 1, carrying that newline in its
    // VALUE, so there is no node for it. Counting nodes alone put the comment
    // one line late, and `carve fmt` then wrote it onto the FOLLOWING line -
    // merging the author's comment text into the next line's text.
    let src = "::: |\n`a\nb`\n%% secret\nd\n:::\n";
    assert_round_trips(src);
    let written = fmt(src);
    assert!(
        !written.contains("secretd"),
        "the comment merged into the next line: {written:?}"
    );
    assert!(
        !written.contains('\\'),
        "wrote a break the author did not: {written:?}"
    );
}

#[test]
fn two_comments_under_one_container_keep_their_order() {
    // The counter is shared across depths, so it has to stay in source order
    // when the walk descends and comes back up.
    let src = "::: |\n*a\n%% one\nb\n%% two\nc*\n:::\n";
    let t = tree(src);
    assert!(t.contains("one") && t.contains("two"), "{t}");
    assert!(
        t.find("one") < t.find("two"),
        "the two notes came back out of order: {t}"
    );
    assert_round_trips(src);
}

#[test]
fn an_open_verbatim_run_still_swallows_the_comment() {
    // THE RULED CASE, and the control on the other direction. An UNCLOSED run
    // carries the emptied line as a newline and there is no place inside it for
    // a node, so the note does not come back - and a fix that put it back
    // anyway would be placing it where the author never wrote it.
    let src = "::: |\n`a\n%% secret\nc\n:::\n";
    assert!(
        !tree(src).contains("secret"),
        "the swallowed note came back: {}",
        tree(src)
    );
    // A bare `%%` is what the other two engines write here, and what this one
    // wrote before and still writes.
    assert!(fmt(src).contains("%%"), "{:?}", fmt(src));
}

#[test]
fn an_indented_comment_line_is_still_verse_text() {
    // §23: `comment_line`'s optional whitespace prefix has nothing to consume in
    // verse, where a leading run is CONTENT. So this line is text, not a
    // comment, at any depth - the descent must not turn it into one.
    let src = "::: |\n*a\n  %% not a comment\nc*\n:::\n";
    let t = tree(src);
    assert!(
        t.contains("not a comment"),
        "the line vanished from the tree: {t}"
    );
    assert!(
        !t.contains("Comment"),
        "an indented line became a comment node: {t}"
    );
}

#[test]
fn the_top_level_spelling_did_not_move() {
    // The control the descent could break: a comment whose boundary is at the
    // stanza's top level was always placed correctly, and still is.
    let src = "::: |\na\n%% secret\nc\n:::\n";
    assert!(tree(src).contains("secret"));
    assert_round_trips(src);
}

#[test]
fn the_soft_to_hard_conversion_moved_and_this_pass_did_not() {
    // THIS ASSERTION USED TO RUN THE OTHER WAY, and that was the point.
    // markup-carve/carve#1351 asked whether §23 hardens a boundary inside a
    // closed inline container; while it was open this file pinned the spelling
    // the engine then had, so that moving it would be a decision rather than a
    // side effect of the comment placement. The ruling landed on Reading A, and
    // this is the visible edit it was left open for.
    //
    // The placement pass above is unchanged by it, which is what the carve-out
    // predicted: it asks where the boundary IS and never what it is called.
    assert_eq!(
        to_html("::: |\n*a\nb\nc*\n:::\n"),
        "<div class=\"line-block\">\n  <p><strong>a<br>\nb<br>\nc</strong></p>\n</div>"
    );
    // The same three lines without the emphasis harden the same way, which is
    // the equality the ruling turned on - one boundary, one break, whatever
    // container it happens to sit in.
    assert_eq!(
        to_html("::: |\na\nb\nc\n:::\n").matches("<br>").count(),
        to_html("::: |\n*a\nb\nc*\n:::\n").matches("<br>").count()
    );
}

#[test]
fn the_comment_publishes_nothing_at_either_depth() {
    // Which is why no render check could see any of this, and why the
    // assertions above are on the tree and the bytes. Stated here so the
    // reasoning is pinned rather than only written down.
    //
    // The comparison is against the SAME document with the comment's text
    // changed, not against one with the line removed - removing it would end the
    // stanza and change the render for a different reason.
    for (a, b) in [
        (
            "::: |\n*a\n%% secret\nc*\n:::\n",
            "::: |\n*a\n%% other\nc*\n:::\n",
        ),
        (
            "::: |\na\n%% secret\nc\n:::\n",
            "::: |\na\n%% other\nc\n:::\n",
        ),
        (
            "::: |\n^[a\n%% secret\nc]\n:::\n",
            "::: |\n^[a\n%% other\nc]\n:::\n",
        ),
    ] {
        assert_eq!(to_html(a), to_html(b), "the comment reached the HTML");
        assert!(!to_html(a).contains("secret"), "{}", to_html(a));
    }
}
