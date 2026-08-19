//! A comment fence hides its body wherever the fence sits, not only at column
//! zero (markup-carve/carve-rs#1047).
//!
//! PART 9 §24 S1 places a line by the column it REACHES and never by its first
//! character, and §28 makes a comment fence's body verbatim and invisible.
//! Neither clause is scoped to column 0, and `resources/examples/edge-cases.md`
//! spells the consequence out under "A definition inside a comment registers
//! nothing": a definition that registered from inside a comment would be
//! invisible in the output and active in the link table at once.
//!
//! That is exactly what a fence at a list item's content column produced here.
//! The block parser reads an indented `%%%` (carve#624) but the two line-based
//! definition pre-passes only ever read the strict column-0 spelling, so they
//! walked into the body and collected from it. carve-js leaves it literal and
//! so does the executable spec oracle; carve-rs and carve-php were the two that
//! drifted, which is why this is a defect against the clause rather than a
//! majority.
//!
//! The corpus pins the rule only for a fence at column 0, which is the gap both
//! engines drifted through - so none of these documents could have gone red.

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src)
}

#[test]
fn a_link_definition_in_an_item_scoped_comment_registers_nothing() {
    assert_eq!(
        html("- item\n  %%%\n  [r]: /url\n  %%%\n\n[r][]\n"),
        "<ul>\n  <li>item</li>\n</ul>\n<p>[r][]</p>"
    );
}

#[test]
fn a_footnote_definition_in_an_item_scoped_comment_registers_nothing() {
    // The worse half, and the one the ticket did not report: a footnote
    // definition collected from a comment does not merely resolve a reference,
    // it emits a whole endnote section nobody wrote.
    let out = html("- item\n  %%%\n  [^f]: note body\n  %%%\n\ntext[^f]\n");
    assert!(!out.contains("doc-endnotes"), "emitted an endnote: {out}");
    assert!(!out.contains("doc-noteref"), "registered a footnote: {out}");
    assert!(
        out.contains("text[^f]"),
        "reference should stay literal: {out}"
    );
}

#[test]
fn a_comment_on_the_marker_line_hides_its_body_too() {
    assert_eq!(
        html("- %%%\n  [r]: /url\n  %%%\n\n[r][]\n"),
        "<ul>\n  <li></li>\n</ul>\n<p>[r][]</p>"
    );
    assert_eq!(
        html("1. %%%\n   [r]: /url\n   %%%\n\n[r][]\n"),
        "<ol>\n  <li></li>\n</ol>\n<p>[r][]</p>"
    );
}

#[test]
fn the_fence_width_does_not_change_the_answer() {
    assert_eq!(
        html("- item\n  %%%%\n  [r]: /url\n  %%%%\n\n[r][]\n"),
        "<ul>\n  <li>item</li>\n</ul>\n<p>[r][]</p>"
    );
}

#[test]
fn a_nested_item_hides_its_comment_body() {
    assert!(
        !html("- a\n  - b\n    %%%\n    [r]: /url\n    %%%\n\n[r][]\n").contains("href=\"/url\"")
    );
}

#[test]
fn an_indented_colon_container_hides_its_comment_body() {
    // At column 0 this already worked, because the fence inside the div is at
    // column 0 too. Put the div in an item and the fence moves off column 0
    // with it, which is the whole defect.
    assert!(
        !html("- item\n  ::: note\n  %%%\n  [r]: /url\n  %%%\n  :::\n\n[r][]\n")
            .contains("href=\"/url\"")
    );
    assert!(
        !html("::: note\n%%%\n[r]: /url\n%%%\n:::\n\n[r][]\n").contains("href=\"/url\""),
        "the column-0 control moved"
    );
}

#[test]
fn a_definition_after_a_contained_comment_still_registers() {
    // The state must END at the closer. Skipping the body is the fix; swallowing
    // the rest of the document is the failure mode that fix invites.
    assert!(
        html("- item\n  %%%\n  hidden\n  %%%\n\n[r]: /url\n\n[r][]\n").contains("href=\"/url\"")
    );
    assert!(
        html("- item\n  %%%\n  hidden\n  %%%\n\n[^f]: note\n\ntext[^f]\n").contains("doc-noteref")
    );
}

#[test]
fn a_closer_back_at_column_zero_does_not_close_an_item_scoped_comment() {
    // The document-wide closer index answers "is there a closer of this length
    // anywhere later", which is the right question only for a column-0 opener.
    // An unterminated fence inside an item degrades to a one-line comment, and
    // the `%%%` two blocks down belongs to no fence - so the definition between
    // them is an ordinary definition and must register. Reading the far closer
    // as this fence's closer lost it.
    let out = html("- item\n  %%%\n  hidden\n\n[r]: /url\n\n%%%\n\n[r][]\n");
    assert!(out.contains("href=\"/url\""), "lost the definition: {out}");
    assert!(
        !out.contains("[r]: /url"),
        "definition leaked as text: {out}"
    );
}

#[test]
fn an_item_scoped_comment_that_never_closes_registers_as_before() {
    // No closer anywhere: the fence degrades to a one-line comment in every
    // engine and the oracle, so the definition under it is a real definition.
    assert!(html("- item\n  %%%\n  [r]: /url\n\n[r][]\n").contains("href=\"/url\""));
}

#[test]
fn an_indented_top_level_fence_is_not_read_as_container_scoped() {
    // A top-level comment's body may sit BELOW its own fence, so its real closer
    // can be at a column this line-based pass cannot bound. Widening the opener
    // to every indented fence mispaired the delimiters here: the pass rejected
    // the true `%%%` / `x` / `%%%` pair, took the second delimiter as an opener,
    // and let `%%% tail` close it - swallowing the definition in between.
    let out = html(" %%%\nx\n  %%%\n\n  - [r]: /u\n  %%% tail\n\n[r][]\n");
    assert!(out.contains("href=\"/u\""), "lost the definition: {out}");
    assert!(!out.contains("[r]: /u"), "definition leaked as text: {out}");
}

#[test]
fn a_fence_below_the_content_column_is_not_the_items_comment() {
    // §24 C3: a comment below an item's content column keeps the item open
    // without being the item's content. Entering opacity there froze the stale
    // content column across the blank that actually ends the list, so the line
    // the block parser reads as a top-level paragraph was stripped to the dead
    // column and registered.
    let out = html("  - x\n  %%%\n\n   %%%\n    [r]: /u\n\n[r][]\n");
    assert!(
        out.contains("<p>[r]: /u</p>"),
        "should stay a top-level paragraph: {out}"
    );
    assert!(
        !out.contains("href=\"/u\""),
        "registered a dead column: {out}"
    );
}

#[test]
fn a_lazy_marker_line_carries_its_comment_and_its_definition_as_text() {
    // A marker after an already-open paragraph is lazy paragraph text, so `- %%%`
    // opens no item, the fence is written inside a paragraph, and the line under
    // it is paragraph text rather than a definition.
    //
    // Reading the opener past the marker gets this right for the first time:
    // main took `- %%%` as an item, collected the definition out of the source,
    // and resolved a reference against a line the reader never sees. Byte-exact
    // against the oracle both ways round.
    assert_eq!(
        html("text\n- %%%\n  [r]: /u\n  %%%\n\n[r][]\n"),
        "<p>text\n-\n[r]: /u</p>\n<p>[r][]</p>"
    );
    let note = html("text\n- %%%\n  [^f]: n\n  %%%\n\nx[^f]\n");
    assert!(!note.contains("doc-endnotes"), "emitted an endnote: {note}");
    assert!(note.contains("[^f]: n"), "ate the line: {note}");
}

#[test]
fn a_list_marker_in_the_body_neither_registers_nor_leaks() {
    // This test used to pin the leak. The pre-pass half was already right - the
    // reference stayed literal - while the block parser ended the comment at the
    // marker and rendered the body, so the shape was half correct and pinned as
    // such, deliberately, so that fixing the block parser would show up here.
    // markup-carve/carve-rs#1053 fixed it: the marker gate in the item's line
    // collector now treats an open comment span as opaque, the way it already
    // treated a code fence and a colon container.
    //
    // No definition needed to show it, and none present:
    assert_eq!(
        html("- item\n  %%%\n  - x\n  y\n  %%%\n\ntail\n"),
        "<ul>\n  <li>item</li>\n</ul>\n<p>tail</p>"
    );

    // With a definition under the marker, both halves now agree: the reference
    // stays literal AND the line it would have come from renders nothing. The
    // old answer resolved neither cleanly - the definition was invisible and the
    // rest of the body was on the page.
    let out = html("- item\n  %%%\n  - x\n  [r]: /u\n  %%%\n\ntail [r][]\n");
    assert!(out.contains("tail [r][]"), "reference resolved: {out}");
    assert!(!out.contains("[r]: /u"), "definition leaked: {out}");
    assert!(!out.contains('x'), "body leaked: {out}");

    // The §28 degradation is untouched: with no closer ahead the opener is an
    // ordinary `%%` line comment, nothing is opaque, and the marker still ends
    // the chunk and opens a sub-list.
    let unclosed = html("- item\n  %%%\n  - x\n  y\n\ntail\n");
    assert!(
        unclosed.contains('x') && unclosed.contains('y'),
        "{unclosed}"
    );
}

#[test]
fn a_definition_in_a_quoted_comment_registers_nothing_either() {
    // This assertion used to run the other way, and deliberately: the quoted
    // spelling was a pinned open question rather than a control. All three
    // engines registered here and only the executable oracle left it literal,
    // so markup-carve/carve-rs#1047 carved it out instead of answering it as a
    // side effect of widening the container strip - pinned so that a later
    // answer would be a visible edit.
    //
    // markup-carve/carve#1341 is that answer, and it is the ruling §28 already
    // implied: the fence's body is verbatim wherever the fence sits, so the
    // prefix that reaches it cannot decide whether a definition registers. The
    // corpus now pins it (`347`), and this edit is the visible one the carve-out
    // was left open for. See `a_definition_in_a_quoted_comment_registers_nothing`
    // for the rest of the shape.
    assert!(!html("> %%%\n> [r]: /url\n> %%%\n\n[r][]\n").contains("href=\"/url\""));
    assert!(!html("- item\n  > %%%\n  > [r]: /url\n  > %%%\n\n[r][]\n").contains("href=\"/url\""));
}

#[test]
fn a_code_fence_in_an_item_was_never_the_problem() {
    // The control for the widening: only the COMMENT opener moved. A verbatim
    // code fence at the same column already hid its definition-shaped line, and
    // still renders it as content rather than swallowing it.
    for src in [
        "- item\n  ```\n  [r]: /url\n  ```\n\n[r][]\n",
        "- item\n  ~~~\n  [r]: /url\n  ~~~\n\n[r][]\n",
    ] {
        let out = html(src);
        assert!(!out.contains("href=\"/url\""), "registered from {src:?}");
        assert!(out.contains("[r]: /url"), "lost the content of {src:?}");
    }
}
