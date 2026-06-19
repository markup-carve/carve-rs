//! Continuation marker in a block quote (Carve, grammar PART 9 §17).
//!
//! A lone `+` at column 0 immediately after a quoted line attaches the
//! following flush-left block to the quote body, with no `>` prefix and no
//! blank line -- the un-prefixed analogue of the list-item continuation marker.
//! It only attaches; a blank line still ends the quote, and a `+` outside any
//! container stays literal text. Output matches carve-php and carve-js.

#[test]
fn attaches_list_to_quote() {
    assert_eq!(
        carve::to_html("> quoted\n+\n- item"),
        "<blockquote>\n  <p>quoted</p>\n  <ul>\n    <li>item</li>\n  </ul>\n</blockquote>"
    );
}

#[test]
fn attaches_fenced_code_to_quote() {
    assert_eq!(
        carve::to_html("> quoted\n+\n```\ncode\n```"),
        "<blockquote>\n  <p>quoted</p>\n  <pre><code>code\n</code></pre>\n</blockquote>"
    );
}

#[test]
fn attaches_table_to_quote() {
    assert_eq!(
        carve::to_html("> quoted\n+\n| a | b |"),
        "<blockquote>\n  <p>quoted</p>\n  <table>\n    <tbody>\n      <tr><td>a</td><td>b</td></tr>\n    </tbody>\n  </table>\n</blockquote>"
    );
}

#[test]
fn two_markers_attach_two_blocks() {
    assert_eq!(
        carve::to_html("> q\n+\n- a\n+\n```\nc\n```"),
        "<blockquote>\n  <p>q</p>\n  <ul>\n    <li>a</li>\n  </ul>\n  <pre><code>c\n</code></pre>\n</blockquote>"
    );
}

#[test]
fn quote_resumes_after_attached_block() {
    assert_eq!(
        carve::to_html("> q\n+\n- item\n> more"),
        "<blockquote>\n  <p>q</p>\n  <ul>\n    <li>item</li>\n  </ul>\n  <p>more</p>\n</blockquote>"
    );
}

#[test]
fn blank_line_before_marker_keeps_it_literal() {
    assert_eq!(
        carve::to_html("> q\n\n+\n- item"),
        "<blockquote><p>q</p></blockquote>\n<p>+\n- item</p>"
    );
}

#[test]
fn indented_marker_is_not_a_continuation_marker() {
    // The indented `+` is not a continuation marker, so it folds into the open
    // quoted paragraph as literal text. The trailing `- item` list marker folds
    // in too (a list marker does not interrupt an open quoted paragraph, the
    // same as at the top level), so the whole thing is one quoted paragraph.
    assert_eq!(
        carve::to_html("> q\n  +\n- item"),
        "<blockquote><p>q\n+\n- item</p></blockquote>"
    );
}
