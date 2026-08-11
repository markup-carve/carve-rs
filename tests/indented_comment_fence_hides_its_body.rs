//! An indented `%%%` fence is a comment, body included (carve-rs#573).
//!
//! The fence was recognized only at column 0, so an indented opener fell to the
//! `%%` line-comment path: the opener and the closer were each consumed as
//! their own one-line comment and every line BETWEEN them rendered as ordinary
//! text. A comment renders nothing, and that has to include what it encloses -
//! showing the contents while hiding the delimiters is the one outcome the
//! construct may never have.
//!
//! carve-js had the same defect at a different position (carve-js#630): there
//! the leak was inside a list item, here under a top-level paragraph.

use carve::to_html;

#[test]
fn an_indented_fence_under_a_paragraph_renders_nothing() {
    assert_eq!(to_html("a\n\n%%%\nx\nb\n%%%\n"), "<p>a</p>");
}

#[test]
fn an_indented_fence_at_the_start_of_a_document_renders_nothing() {
    assert_eq!(to_html("%%%\nx\nb\n%%%\n"), "");
}

#[test]
fn an_indented_fence_inside_a_list_item_keeps_the_item_open() {
    // The comment is invisible and closes nothing: `tail` is still item
    // content, the shape carve-rs#572 settled for a column-0 comment.
    assert_eq!(
        to_html("- a\n+\n%%%\nn\nx\n%%%\n+\ntail\n"),
        "<ul>\n  <li>a\n    tail\n  </li>\n</ul>"
    );
}

#[test]
fn an_indented_closer_closes_an_indented_opener() {
    // Leading whitespace is not part of the delimiter; the `%` run length is.
    assert_eq!(to_html("a\n\n%%%\nx\nb\n%%%\n\nc\n"), "<p>a</p>\n<p>c</p>");
}

#[test]
fn an_unclosed_indented_fence_opens_no_block() {
    // PART 9 section 28: without a closer it is not a fenced comment, so the
    // following blocks still render instead of being swallowed to EOF.
    assert_eq!(to_html("a\n\n%% % x\n\nb\n"), "<p>a</p>\n<p>b</p>");
}
