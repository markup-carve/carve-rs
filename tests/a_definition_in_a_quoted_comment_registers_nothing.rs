//! A comment fence reached through a QUOTE hides its body too
//! (markup-carve/carve#1341, markup-carve/carve-rs#1078).
//!
//! `markup-carve/carve#1309` ruled that a definition inside a comment fence is
//! not registered at every column a fence can sit at, and the corpus pinned two
//! spellings: the column-0 one and the indented one inside a list item. The
//! quoted one was never pinned and all three engines leaked through it, this one
//! for two definition kinds.
//!
//! The tell that it was leakage rather than a competing reading of §28 is that
//! it sorted definitions by KIND. carve-js registered the link reference and not
//! the footnote; this engine registered both; the abbreviation collector neither.
//! A rule an engine was following would not sort definitions by kind.
//!
//! Mechanically it was one question asked of the wrong index. Both definition
//! pre-passes gated a container-scoped fence on the DOCUMENT-WIDE closer index
//! first, and that index reads raw lines, where a `> %%%` closer wears its quote
//! marker and matches nothing - so the quoted fence read as unterminated to the
//! pre-passes while the block parser consumed it. `markup-carve/carve-rs#1052`
//! had already fixed the indented spelling with a container bound; the quote
//! marker is that problem one prefix over.

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src)
}

#[test]
fn a_link_definition_in_a_quoted_comment_registers_nothing() {
    // Corpus `347`. The blockquote is EMPTY either way - the fence was always
    // consumed as a comment - so only the registration moves.
    assert_eq!(
        html("> %%%\n> [r]: /url\n> %%%\n\nSee [r][].\n"),
        "<blockquote>\n\n</blockquote>\n<p>See [r][].</p>"
    );
}

#[test]
fn a_footnote_definition_in_a_quoted_comment_registers_nothing() {
    // Corpus `347-2`, and the half carve-js never had: a footnote collected from
    // a comment does not merely resolve a reference, it emits a whole endnote
    // section nobody wrote.
    assert_eq!(
        html("> %%%\n> [^f]: note\n> %%%\n\nSee [^f].\n"),
        "<blockquote>\n\n</blockquote>\n<p>See [^f].</p>"
    );
}

#[test]
fn the_abbreviation_kind_is_unchanged_by_this() {
    // NOT a fix and not coverage. PART 12 §7 recognizes an abbreviation
    // definition only as a direct child of the document, so a quoted one defines
    // nothing whether a fence hides it or not. It is here as the control on the
    // OTHER direction: a change that suppressed by container rather than by
    // comment would move these two together, and they must not move.
    assert!(!html("> %%%\n> *[ab]: abbrev\n> %%%\n\nSee ab.\n").contains("<abbr"));
    assert!(!html("> *[ab]: abbrev\n\nSee ab.\n").contains("<abbr"));
    assert!(
        html("*[ab]: abbrev\n\nSee ab.\n").contains("<abbr title=\"abbrev\">ab</abbr>"),
        "the document-level control moved"
    );
}

#[test]
fn a_quoted_definition_with_no_comment_still_registers() {
    // Corpus `347-3`, and the shape an over-suppressing fix breaks. A definition
    // inside a quote registers document-wide, per kind, and nothing here changes
    // that: the comment is what defers a definition, never the quote.
    let out = html("> [r]: /url\n> [^f]: note\n\nSee [r][] [^f].\n");
    assert!(out.contains("href=\"/url\""), "lost the reference: {out}");
    assert!(out.contains("doc-noteref"), "lost the footnote: {out}");
}

#[test]
fn the_already_pinned_spellings_did_not_move() {
    // The column-0 and item-content-column spellings are pinned and unanimous.
    // Breaking either would be worse than the gap this closes.
    for src in [
        "%%%\n[r]: /url\n%%%\n\nSee [r][].\n",
        "- item\n  %%%\n  [r]: /url\n  %%%\n\nSee [r][].\n",
    ] {
        assert!(!html(src).contains("href=\"/url\""), "moved: {src:?}");
    }
    for src in [
        "%%%\n[^f]: note\n%%%\n\nSee [^f].\n",
        "- item\n  %%%\n  [^f]: note\n  %%%\n\nSee [^f].\n",
    ] {
        assert!(!html(src).contains("doc-noteref"), "moved: {src:?}");
    }
}

#[test]
fn the_fence_still_consumes_its_body() {
    // Registration is the ONLY thing that moves. A fix that declined the fence
    // instead of declining the definition would put the commented-out lines back
    // on the page, which is the opposite defect and a worse one.
    assert_eq!(
        html("> q\n> %%%\n> x\n> %%%\n> body\n"),
        "<blockquote>\n  <p>q</p>\n  <p>body</p>\n</blockquote>"
    );
    assert!(!html("> %%%\n> [r]: /url\n> %%%\n\nSee [r][].\n").contains("[r]: /url"));
}

#[test]
fn a_closer_that_is_not_in_the_same_quote_does_not_close_the_fence() {
    // Widening the opener alone would be worse than the gap. Each of these
    // fences is UNTERMINATED - §28 degrades it to a one-line comment - so the
    // definition below it is an ordinary definition the block parser publishes,
    // and entering the region on a far closer would swallow it.
    //
    // The five ways a closer can fail to be in the same quote:
    for (src, why) in [
        ("> %%%\n> [r]: /url\n\nSee [r][].\n", "no closer at all"),
        (
            "> %%%\n> [r]: /url\n\n> %%%\n\nSee [r][].\n",
            "below a blank, so a different quote",
        ),
        (
            "> %%%\n> [r]: /url\n%%%\n\nSee [r][].\n",
            "back at column 0, outside the quote",
        ),
        (
            "> %%%\n> [r]: /url\n> > %%%\n\nSee [r][].\n",
            "one quote deeper, which the parser reads as content",
        ),
        (
            "> %%%%\n> [r]: /url\n> %%%\n\nSee [r][].\n",
            "the wrong width",
        ),
    ] {
        assert!(
            html(src).contains("href=\"/url\""),
            "lost a definition whose fence never closed ({why})",
        );
    }
}

#[test]
fn a_blank_line_is_the_whole_bound_for_a_quoted_scope() {
    // A blockquote does not survive a blank line, so `> a` / blank / `> b` is
    // two quotes and a run after the blank belongs to the second. That is why
    // the quoted scope takes the blank as its bound where the column scope takes
    // a dedent - there a dedented line really can be a lazy continuation.
    //
    // Both quotes here hold an unterminated opener, so both definitions register
    // rather than one region spanning the blank and eating them.
    let out = html("> %%%\n> [r]: /one\n\n> %%%\n> [s]: /two\n\nSee [r][] and [s][].\n");
    assert!(out.contains("href=\"/one\""), "lost the first: {out}");
    assert!(out.contains("href=\"/two\""), "lost the second: {out}");
}

#[test]
fn a_definition_after_a_quoted_comment_still_registers() {
    // The region must END at its closer. Skipping the body is the fix;
    // swallowing the rest of the document is the failure mode that fix invites.
    assert!(html("> %%%\n> hidden\n> %%%\n\n[r]: /url\n\nSee [r][].\n").contains("href=\"/url\""));
    assert!(html("> %%%\n> hidden\n> %%%\n\n[^f]: note\n\nSee [^f].\n").contains("doc-noteref"));
}

#[test]
fn the_answer_does_not_depend_on_unrelated_text_later_in_the_document() {
    // The tell that named the mechanism. The raw closer index answers a
    // DOCUMENT-WIDE question, so a stray column-0 `%%%` anywhere below got the
    // same quoted fence past the gate and it then answered correctly - which is
    // why the already-pinned spellings passed while the reported one did not.
    //
    // Both of these must now give the same answer, because they are the same
    // question.
    let bare = html("> %%%\n> [r]: /url\n> %%%\n\nSee [r][].\n");
    let with_stray = html("> %%%\n> [r]: /url\n> %%%\n\nSee [r][].\n\n%%%\n");
    assert!(!bare.contains("href=\"/url\""), "{bare}");
    assert!(!with_stray.contains("href=\"/url\""), "{with_stray}");
}

#[test]
fn a_deeper_quote_hides_its_own_comment_body() {
    // Depth is carried, not just quotedness: a `> > %%%` opens at depth 2 and is
    // closed by a `> > %%%`, not by the `> %%%` that ends the outer quote.
    assert!(!html("> > %%%\n> > [r]: /url\n> > %%%\n\nSee [r][].\n").contains("href=\"/url\""));
    assert!(!html("> > %%%\n> > [^f]: note\n> > %%%\n\nSee [^f].\n").contains("doc-noteref"));
}

#[test]
fn a_quote_inside_an_item_hides_its_comment_body_too() {
    // Both prefixes at once. The quote is the innermost container, so the blank
    // line bounds it whatever the item around it does.
    assert!(
        !html("- item\n  > %%%\n  > [r]: /url\n  > %%%\n\nSee [r][].\n").contains("href=\"/url\"")
    );
    assert!(
        !html("- item\n  > %%%\n  > [^f]: note\n  > %%%\n\nSee [^f].\n").contains("doc-noteref")
    );
}

#[test]
fn a_quote_that_opens_on_the_marker_line_hides_its_comment_body() {
    // The same fence one spelling over, and the one the first version of this
    // change still leaked: reading only the LEADING blockquote markers left
    // `- > %%%` at depth 0, so neither the quoted arm nor the column arm - which
    // does not strip a `>` - recognized it, while the block parser consumed it
    // exactly as it consumes `  > %%%`.
    for src in [
        "- > %%%\n  > [r]: /url\n  > %%%\n\nSee [r][].\n",
        "1. > %%%\n   > [r]: /url\n   > %%%\n\nSee [r][].\n",
        "- - > %%%\n    > [r]: /url\n    > %%%\n\nSee [r][].\n",
    ] {
        let out = html(src);
        assert!(!out.contains("href=\"/url\""), "registered from {src:?}");
        assert!(!out.contains("[r]: /url"), "body leaked from {src:?}");
    }
    assert!(!html("- > %%%\n  > [^f]: note\n  > %%%\n\nSee [^f].\n").contains("doc-noteref"));
}

#[test]
fn a_closer_that_has_left_the_item_does_not_close_a_marker_line_quote() {
    // Why the quoted bound carries a column as well as a depth, and the case the
    // blank line cannot answer. NO blank line here, so the blank half of the
    // bound never fires: the `> %%%` back at column 0 is one marker deep, the
    // same as the `- > %%%` that opened, and the depth alone reads it as this
    // fence's closer. It has left the item, so the block parser reads the opener
    // as unterminated and publishes the definition under it. Taking the far
    // closer suppressed that definition AND put it back on the page as quoted
    // text, which is a worse defect than the one this change closes.
    let out = html("- > %%%\n  > [r]: /url\n> %%%\n\nSee [r][].\n");
    assert!(out.contains("href=\"/url\""), "lost the definition: {out}");
    assert!(
        !out.contains("[r]: /url"),
        "definition leaked as text: {out}"
    );

    // The control on the same pair: at column 0 there is nothing to dedent below,
    // so the blank line stays the whole bound there and nothing about a quote
    // written at the document level changes.
    assert!(html("> %%%\n> [r]: /url\n\n> %%%\n\nSee [r][].\n").contains("href=\"/url\""));
}

#[test]
fn a_quoted_code_fence_was_never_the_problem() {
    // The control for the widening: only the COMMENT opener moved. A verbatim
    // code fence in the same quote already hid its definition-shaped line, and
    // still renders it as content rather than swallowing it.
    for src in [
        "> ```\n> [r]: /url\n> ```\n\nSee [r][].\n",
        "> ~~~\n> [r]: /url\n> ~~~\n\nSee [r][].\n",
    ] {
        let out = html(src);
        assert!(!out.contains("href=\"/url\""), "registered from {src:?}");
        assert!(out.contains("[r]: /url"), "lost the content of {src:?}");
    }
}
