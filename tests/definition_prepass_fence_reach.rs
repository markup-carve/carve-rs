//! A fence run that opens no fence must not hide a definition from the
//! collecting pre-passes (§10 I4, markup-carve/carve-rs#2096).
//!
//! Expectations are the executable spec's own reading, taken from
//! `scripts/spec/layout.mjs` plus `scripts/spec/html.mjs` at markup-carve/carve
//! 38829a97. The last two cases are the controls: with no paragraph above, and
//! with a closer ahead, the fence is real and its body stays verbatim.

#[test]
fn a_comment_above_an_unterminated_fence_keeps_collecting() {
    assert_eq!(
        carve::to_html("%%\n```\n[d]: u\n"),
        "<pre><code>[d]: u\n</code></pre>"
    );
}

#[test]
fn a_backtick_run_after_a_paragraph_collects_the_footnote() {
    assert_eq!(
        carve::to_html("t\n```\n[^f]: t ~\n"),
        "<p>t\n<code></code></p>"
    );
}

#[test]
fn another_opener_after_a_paragraph_collects_the_footnote() {
    assert_eq!(
        carve::to_html(")\n```\n[^f]: : |\n"),
        "<p>)\n<code></code></p>"
    );
}

#[test]
fn a_tilde_run_after_a_paragraph_collects_the_footnote() {
    assert_eq!(carve::to_html("]\n~~~\n\n[^f]: )\n"), "<p>]\n~~~</p>");
}

#[test]
fn a_terminated_fence_after_a_paragraph_still_hides_one() {
    assert_eq!(
        carve::to_html("x\n```\n[d]: u\n```\n"),
        "<p>x</p>\n<pre><code>[d]: u\n</code></pre>"
    );
}

#[test]
fn a_fence_after_a_blank_still_hides_a_definition() {
    assert_eq!(
        carve::to_html("x\n\n```\n[d]: u\n```\n"),
        "<p>x</p>\n<pre><code>[d]: u\n</code></pre>"
    );
}
