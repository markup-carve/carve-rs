//! A code or raw fence opened PAST a quoted list item's or footnote's content
//! column is still that host's fence. Section 24 C3 asks a processor to name such
//! an opener rather than refuse it, so the host holds its payload exactly as it
//! holds a canonical one, and a flush-left line after it reaches no open paragraph
//! (markup-carve/carve#2563, corpus category 519, carve-rs#2170).
//!
//! The opener used to be parked in a pending slot instead, which left the host's
//! paragraph open over the payload, so the flush-left line folded into it and the
//! quote swallowed it. Ported from carve-js#2356, which landed first and is the
//! reference for the shared cause.

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

/// Corpus 519's first document. The flush-left line ends the quote instead of
/// joining the payload.
#[test]
fn a_flush_line_after_a_shifted_fence_ends_the_quote() {
    assert_eq!(
        html("> - a\n>\n>     ```\n>     x\nflush\n"),
        "<blockquote>\n  <ul>\n    <li>a\n      <pre><code>x\n</code></pre>\n    </li>\n  </ul>\n</blockquote>\n<p>flush</p>"
    );
}

/// Corpus 519-2. What follows the flush-left line is read outside the item too,
/// and the fence's own closer never arrives inside it.
#[test]
fn the_lines_after_the_flush_line_are_outside_the_item() {
    assert_eq!(
        html("> - a\n>\n>     ~~~\n>     x\nz\n>     b\n>     ~~~\n"),
        "<blockquote>\n  <ul>\n    <li>a\n      <pre><code>x\n</code></pre>\n    </li>\n  </ul>\n</blockquote>\n<p>z</p>\n<blockquote><p>b\n~~~</p></blockquote>"
    );
}

/// Corpus 519-3, at depth two and with a RAW fence, so the rule is the host's and
/// not the fence kind's.
#[test]
fn a_shifted_raw_fence_at_depth_two_reads_the_same_way() {
    assert_eq!(
        html("> > - a\n> >\n> >     ```=html\n> >     x\nflush\n"),
        "<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n        x\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p>flush</p>"
    );
}

/// A closer at the host's own column ends a shifted opener, which is the property
/// that keeps an over-indented opener from consuming a dedented closer.
#[test]
fn a_closer_at_either_column_ends_the_fence() {
    for source in [
        // closer at the opener's column
        "> - a\n>\n>     ```\n>     x\n>     ```\n>\n> tail\n",
        // closer at the host's content column
        "> - a\n>\n>     ```\n>     x\n>   ```\n",
    ] {
        let out = html(source);
        assert!(
            out.contains("<pre><code>x\n</code></pre>"),
            "{source:?} -> {out}"
        );
        assert!(
            !out.contains("```"),
            "{source:?} kept a fence marker: {out}"
        );
    }
}

/// The canonical opener, at the host's content column, is unchanged. This is the
/// A container ending an empty fence contributes no payload characters.
#[test]
fn an_opener_at_the_content_column_is_unchanged() {
    assert_eq!(
        html("> - a\n>\n>   ```\n>   x\nflush\n"),
        "<blockquote>\n  <ul>\n    <li>a\n      <pre><code>x\n</code></pre>\n    </li>\n  </ul>\n</blockquote>\n<p>flush</p>"
    );
}

/// An unterminated fence that collects no payload line still owns a final line
/// break: it runs to the end of its container and the end supplies it. Corpus 276
/// pins that, and this is here so the fix cannot take it away.
#[test]
fn an_unterminated_empty_fence_has_no_payload() {
    assert!(html("- ```\nx\n```\n").contains("<pre><code></code></pre>"));
    assert!(html("> ```\nx\n```\n").contains("<pre><code></code></pre>"));
}

/// A quoted host with no fence at all still takes its lazy line, so the change did
/// not turn every flush-left line into a quote terminator.
#[test]
fn a_quoted_paragraph_still_takes_its_lazy_line() {
    assert_eq!(
        html("> - a\n  b\n"),
        "<blockquote>\n  <ul>\n    <li>a\nb</li>\n  </ul>\n</blockquote>"
    );
}
