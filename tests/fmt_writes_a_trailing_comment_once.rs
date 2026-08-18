//! What the removed re-grafting pass actually did, measured (carve-rs#1076).
//!
//! carve-rs#1083 removed `restore_inline_comments` and argued the removal
//! analytically: the writer emits `InlineNode::Comment` itself, so the intended
//! formatted line already ends in the marker and is therefore not equal to the
//! rendering of the part before it - leaving the graft inert or harmful, never
//! corrective. That PR states plainly what it did NOT claim:
//!
//! > a corpus-wide byte comparison of `to_carve` with and without the helper.
//! > I started a full suite run with it neutered and killed it once the fix made
//! > it moot, so there is no completed run behind that.
//!
//! This file is that run, plus the shape it turned up. The measurement, taken on
//! a build of `9275ecdd` (the commit before carve-rs#1083) against the same build
//! with the pass neutered:
//!
//! - Over the 1175 corpus documents, `carve fmt` output was byte-identical with
//!   and without the pass, and the graft never fired once.
//! - Over 140 documents crossing ten container contexts with fourteen
//!   comment-bearing line shapes, byte-identical again.
//! - Over 2639 documents made by injecting ` %% note` into one line of a corpus
//!   document, the graft fired 20 times: 20 duplicates written, 0 comments
//!   rescued. Inert or harmful, never corrective - now measured rather than
//!   argued.
//!
//! TWO of those 20 landed on a FRONTMATTER delimiter, which is worse than a
//! duplicated note and is the case this file exists for: it rewrote the block's
//! opening `---`, so the document stopped holding the frontmatter the author
//! wrote. carve-rs#1083 fixed it along with everything else the pass did, and
//! nothing pinned it.
//!
//! The reported line-block document and its paragraph spelling are pinned by
//! carve-rs#1083 already; the spellings here are the ones that were not.

use carve::{parse, to_carve, to_html};

fn tree(src: &str) -> String {
    format!("{:?}", parse(src))
}

#[test]
fn a_frontmatter_delimiter_does_not_collect_a_comment() {
    // The graft fired at line 0 with marker `---`, appending the comment to the
    // block's OPENING delimiter:
    //
    //     ---                         --- %% note
    //     title: T          became
    //     --- %% note                 title: T
    //                                 --- %% note
    let src = "---\ntitle: T\n--- %% note\n\nBody.\n";
    let out = to_carve(src);
    assert!(
        !out.starts_with("--- %%"),
        "the opening delimiter was rewritten: {out:?}"
    );
    assert_eq!(
        out.matches("%% note").count(),
        1,
        "the comment was written twice: {out:?}"
    );
}

#[test]
fn the_repeated_line_shape_in_the_containers_that_were_not_pinned() {
    // carve-rs#1083 pins the line-block document and the paragraph spelling. The
    // graft matched ANY formatted line, so every container where a line can
    // repeat reached it; these are the rest of them.
    //
    // Measured against a build of `9275ecdd`, the commit before carve-rs#1083:
    // the quote, the list, the div and the description list each wrote the note
    // TWICE there and write it once now. The heading row was already correct
    // before the fix - `# a` renders as `# a`, which never equals the `a` the
    // marker is rendered from - so it is a control on the removal rather than a
    // case it repaired, and it is kept for that.
    for src in [
        "> a\n\n> a %% note\n",
        "- a\n- a %% note\n",
        "::: n\na\na %% note\n:::\n",
        ":: t\n:  a\n\n:: u\n:  a %% note\n",
        "# a\n\na %% note\n",
    ] {
        let out = to_carve(src);
        assert_eq!(
            out.matches("%% note").count(),
            1,
            "the comment was written {} times for {src:?}: {out:?}",
            out.matches("%% note").count()
        );
    }
}

#[test]
fn the_document_gains_no_comment_node_it_did_not_have() {
    // The assertion neither existing gate could make. A comment renders to
    // nothing, so `to_html(fmt(x)) == to_html(x)` held; the duplicate is itself
    // a comment, so `fmt(fmt(x)) == fmt(x)` held too. Only the tree shows it.
    //
    // `Comment {` rather than `Comment`: the enum's Debug prints the variant AND
    // the struct, so the bare word counts each node twice.
    for src in [
        "---\ntitle: T\n--- %% note\n\nBody.\n",
        "> a\n\n> a %% note\n",
        "- a\n- a %% note\n",
    ] {
        let before = tree(src).matches("Comment {").count();
        let after = tree(&to_carve(src)).matches("Comment {").count();
        assert_eq!(
            before, after,
            "formatting changed how many comments {src:?} holds"
        );
    }
}

#[test]
fn a_trailing_comment_is_still_written_in_every_container() {
    // THE CONTROL ON THE REMOVAL. If the writer were not the thing carrying
    // these, deleting the pass would drop them - and every assertion above would
    // still be green, because they all count DOWN from a duplicate.
    //
    // `fmt_comment_blocks` holds this for a paragraph and a blockquote; the rest
    // are here, because the claim carve-rs#1083 rests on is about the WRITER and
    // is therefore a claim about every container it writes.
    for src in [
        "text %% a note\n\nmore\n",
        "> text %% a note\n",
        "- text %% a note\n",
        "# h %% a note\n",
        "::: |\na %% a note\n:::\n",
        "::: n\na %% a note\n:::\n",
        "| a %% a note |\n",
    ] {
        let out = to_carve(src);
        assert!(
            out.contains("%% a note"),
            "the inline comment was dropped for {src:?}: {out:?}"
        );
    }
}

#[test]
fn formatting_stays_idempotent_and_render_preserving_here() {
    // Both properties held THROUGH the defect, which is why they are controls
    // rather than evidence - but a removal is exactly the change that could
    // break them, so they are asserted on the shapes this file adds.
    for src in [
        "---\ntitle: T\n--- %% note\n\nBody.\n",
        "> a\n\n> a %% note\n",
        "- a\n- a %% note\n",
        "::: |\na %% a note\n:::\n",
    ] {
        let once = to_carve(src);
        assert_eq!(to_carve(&once), once, "fmt is not idempotent on {src:?}");
        assert_eq!(
            to_html(&once),
            to_html(src),
            "formatting changed the render of {src:?}"
        );
    }
}
