//! An unterminated code fence is not an opaque span.
//!
//! A fence opener with no closer ahead does not open a code block that swallows
//! the structure around it. Inside a `:::` body that matters twice over: the
//! container's own closing `:::` must stay structural, and text written after
//! it must not be dragged inside.
//!
//! Decided in markup-carve/carve#515, where the three engines had split 2-1 -
//! carve-js and carve-rs let the fence win and consume the rest of the
//! document, carve-php let the div closer win. carve-php's rule was adopted:
//! it is what PART 9 §10 I4 already says one level up (an opener with no closer
//! is not a fence), and it bounds the blast radius of a single typo.
//!
//! The settled case is unaffected: a CLOSED fence stays opaque, so `:::` inside
//! one is still literal text.

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn unterminated_fence_does_not_swallow_the_div_closer() {
    let html = squash(&to_html("::: note\n```\nx\n:::\nafter\n"));

    assert_eq!(
        html,
        "<aside class=\"admonition note\"> <pre><code>x </code></pre> </aside> <p>after</p>"
    );
}

#[test]
fn unterminated_fence_after_a_blockquote_does_not_invent_a_div() {
    // carve-rs#458: the orphaned `:::` was reparsed as an empty div, which then
    // captured every following block. The `<div>` had no source text at all.
    let html = squash(&to_html("::: note\n> a\n```\n:::\nafter\n"));

    assert!(
        !html.contains("<div>"),
        "invented a div with no source: {html}"
    );
    assert!(
        html.ends_with("</aside> <p>after</p>"),
        "`after` was written outside the aside: {html}"
    );
}

#[test]
fn a_closed_fence_is_still_opaque() {
    // The case that must not regress: showing Carve syntax in a code block.
    let html = squash(&to_html("::: note\n````\n:::\n````\nafter\n:::\n"));

    assert_eq!(
        html,
        "<aside class=\"admonition note\"> <pre><code>::: </code></pre> <p>after</p> </aside>"
    );
}

#[test]
fn a_lone_unterminated_fence_still_opens_a_code_block() {
    // I4 gates INTERRUPTION. With nothing to interrupt, an unterminated fence
    // still opens a code block that runs to the end - unchanged, and the same
    // in all three engines.
    assert_eq!(squash(&to_html("```\nx\n")), "<pre><code>x </code></pre>");
    assert_eq!(
        squash(&to_html("> ```\n")),
        "<blockquote> <pre><code> </code></pre> </blockquote>"
    );
}
