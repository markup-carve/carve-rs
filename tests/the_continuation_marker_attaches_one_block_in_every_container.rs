//! The continuation marker is ONE operation in every container.
//!
//! markup-carve/carve#1782 (PART 9 §17 L3/L4): a lone `+` transfers ownership
//! of the NEXT flush-left block to the container whose marker column it sits
//! at: one block, whatever kind it is. A list item, a block quote, a footnote
//! body and a definition description all take the marker, and all take it the
//! same way.
//!
//! This engine spelled the measurement five times and narrowed it in two of
//! them (markup-carve/carve-rs#1428). The list item narrows structurally - it
//! parses one block out of the run and advances by what that consumed - and the
//! block quote narrowed by measuring; the footnote body and the two
//! definition-description spellings took the whole run up to the boundary. So
//! L3's own example - `+` / `para` / `> q` - left the quote outside a list item
//! and pulled it into a note and a description. The narrowing was not a
//! decision any of them had made differently; it had simply been written twice.
//!
//! A divergence in the other direction sat in the fifth. The quote's boundary
//! closure ended the run at any line carrying a `>` prefix, so the one kind of
//! block the marker refused to attach was the kind written with the container's
//! own
//! marker: `> a` / `+` / `> q` attached NOTHING and folded `q` into the quoted
//! paragraph, where the same clause says the marker only ever attaches.
//!
//! BOTH EDGES ARE PINNED HERE, and they fail to different mutations. The
//! headline set below is the same document in all four containers plus the
//! nested quote - corpus
//! `427-the-continuation-marker-attaches-one-block-in-every-container`, which
//! this repo's spec pin does not yet carry, so the documents are written out
//! rather than swept. The boundary set underneath asserts that the reach still
//! stops at exactly ONE block: a marker that attached everything up to the
//! boundary would pass every headline case in a note and a description and
//! still be wrong, and a marker narrowed to one LINE would pass the nested
//! quote and break every multi-line block.

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

// ---------------------------------------------------------------------------
// The same document in every container (corpus 427)
// ---------------------------------------------------------------------------

#[test]
fn a_list_item_attaches_one_block() {
    // The clause's own example, and the one spelling that already read it
    // correctly: the marker takes `para`, the quote stays outside the item.
    assert_eq!(
        html("- a\n+\npara\n> q\n"),
        "<ul>\n  <li>a\n    para\n  </li>\n</ul>\n<blockquote><p>q</p></blockquote>"
    );
}

#[test]
fn a_footnote_body_attaches_one_block() {
    // The same document one container over. The note ends after `para`, so the
    // quote is the document's own block - and it renders where it was written,
    // ahead of the paragraph that references the note.
    assert_eq!(
        html("[^n]: a\n+\npara\n> q\n\nsee[^n]\n"),
        "<blockquote><p>q</p></blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>a</p>\n      <p>para<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_definition_description_attaches_one_block() {
    // And in a description, where the quote used to land inside the `<dd>`.
    assert_eq!(
        html(":: t\n:  a\n+\npara\n> q\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>a</p>\n    <p>para</p>\n  </dd>\n</dl>\n<blockquote><p>q</p></blockquote>"
    );
}

#[test]
fn a_block_quote_attaches_a_following_quote_line() {
    // The marker only ever attaches. A `>` line is a block like any other, so it
    // nests rather than folding into the quoted paragraph above it.
    assert_eq!(
        html("> a\n+\n> q\n"),
        "<blockquote>\n  <p>a</p>\n  <blockquote><p>q</p></blockquote>\n</blockquote>"
    );
}

// ---------------------------------------------------------------------------
// The reach stops at exactly one block
// ---------------------------------------------------------------------------

#[test]
fn a_footnote_body_stops_at_the_end_of_the_attached_block() {
    // The fence is the one block; the paragraph behind it is the document's.
    // A reach that ran to the boundary would put `para` in the note.
    assert_eq!(
        html("[^n]: a\n+\n```\ncode\n```\npara\n\nsee[^n]\n"),
        "<p>para</p>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>a</p>\n      <pre><code>code\n</code></pre>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_definition_description_stops_at_the_end_of_the_attached_block() {
    // The same boundary in a description.
    assert_eq!(
        html(":: t\n:  a\n+\n```\ncode\n```\npara\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>a</p>\n    <pre><code>code\n</code></pre>\n  </dd>\n</dl>\n<p>para</p>"
    );
}

#[test]
fn an_attached_block_may_span_many_lines() {
    // ONE BLOCK IS NOT ONE LINE. A narrowing that counted lines instead of
    // blocks would split the fence and leave `code` and the closing fence
    // outside the quote.
    assert_eq!(
        html("> a\n+\n```\ncode\n```\n"),
        "<blockquote>\n  <p>a</p>\n  <pre><code>code\n</code></pre>\n</blockquote>"
    );
}

#[test]
fn a_two_line_quote_is_one_attached_block() {
    // The same on the kind whose boundary test was removed: both quoted lines
    // are one block quote, and the marker takes the whole of it.
    assert_eq!(
        html("> a\n+\n> q\n> r\n"),
        "<blockquote>\n  <p>a</p>\n  <blockquote><p>q\nr</p></blockquote>\n</blockquote>"
    );
}

#[test]
fn a_quote_line_after_the_attached_block_still_continues_the_quote() {
    // Removing the boundary test did not make a following `>` line content of
    // the attached paragraph. The narrowing stops in front of it, and the
    // quote's own loop reads it on the next turn - so `q` is quoted prose,
    // not a nested quote.
    assert_eq!(
        html("> a\n+\npara\n> q\n"),
        "<blockquote>\n  <p>a</p>\n  <p>para</p>\n  <p>q</p>\n</blockquote>"
    );
}

#[test]
fn a_further_marker_starts_a_second_attachment() {
    // A further `+` ends the first attachment and opens its own, so a quote can
    // take two attached blocks in a row.
    assert_eq!(
        html("> a\n+\n> q\n+\npara\n"),
        "<blockquote>\n  <p>a</p>\n  <blockquote><p>q</p></blockquote>\n  <p>para</p>\n</blockquote>"
    );
}
