//! Raw blocks use djot's `=FORMAT` syntax (```=html), matching carve-js /
//! carve-php. The former `raw FORMAT` keyword form was removed. The code-fence
//! language token accepts `/` so MIME-like tags stay a single token.

#[test]
fn raw_block_passes_through_matching_format() {
    assert_eq!(
        carve::to_html("```=html\n<custom-el>V</custom-el>\n```"),
        "<custom-el>V</custom-el>"
    );
}

#[test]
fn all_blank_raw_payload_lines_are_preserved() {
    let one = carve::parse("```=html\n\n```\n");
    let two = carve::parse("```=html\n\n\n```\n");
    let carve::BlockNode::RawBlock(one) = &one.children[0] else {
        panic!("expected raw block");
    };
    let carve::BlockNode::RawBlock(two) = &two.children[0] else {
        panic!("expected raw block");
    };
    assert_eq!(one.content, "\n");
    assert_eq!(two.content, "\n\n");
}

#[test]
fn raw_block_drops_non_matching_format() {
    assert_eq!(carve::to_html("```=latex\n\\emph{x}\n```"), "");
}

#[test]
fn raw_block_accepts_leading_whitespace_before_eq() {
    assert_eq!(carve::to_html("``` =html\n<b>x</b>\n```"), "<b>x</b>");
}

#[test]
fn eq_with_space_before_format_is_not_raw() {
    // ```= html is not a raw block; the line opens an inline code span.
    assert_eq!(
        carve::to_html("```= html\n<b>x</b>\n```"),
        "<p><code>= html\n&lt;b&gt;x&lt;/b&gt;\n</code></p>"
    );
}

#[test]
fn removed_raw_keyword_form_is_not_raw() {
    assert_eq!(
        carve::to_html("```raw html\n<b>x</b>\n```"),
        "<p><code>raw html\n&lt;b&gt;x&lt;/b&gt;\n</code></p>"
    );
}

#[test]
fn language_token_accepts_slash() {
    assert_eq!(
        carve::to_html("```text/html\nx\n```"),
        "<pre><code class=\"language-text/html\">x\n</code></pre>"
    );
}

#[test]
fn language_token_accepts_leading_slash() {
    assert_eq!(
        carve::to_html("```/html\nx\n```"),
        "<pre><code class=\"language-/html\">x\n</code></pre>"
    );
}

/// CARVE-P2-022 ties the payload's newline to the format matching the output
/// format, so a raw block the HTML target drops contributes no line at all --
/// not even the separator the two root loops push before an ordinary block.
#[test]
fn a_dropped_raw_block_owes_no_line_at_the_document_root() {
    assert_eq!(
        carve::to_html("a\n\n```=latex\n```\n\nb\n"),
        "<p>a</p>\n<p>b</p>"
    );
}

/// One blank payload line is a second source shape, and a dropped target
/// answers it the same way: the payload never reaches the output.
#[test]
fn a_dropped_raw_block_with_one_blank_payload_line_owes_no_line() {
    assert_eq!(
        carve::to_html("a\n\n```=latex\n\n```\n\nb\n"),
        "<p>a</p>\n<p>b</p>"
    );
}

/// A heading `<section>` is a rendering artifact: its children are still the
/// document's own blocks (CARVE-P9-073), and the section loop owes the dropped
/// block no line either.
#[test]
fn a_dropped_raw_block_owes_no_line_inside_a_heading_section() {
    assert_eq!(
        carve::to_html("a\n\n## h\n\nc\n\n```=latex\n```\n\nb\n"),
        "<p>a</p>\n<section id=\"h\">\n  <h2>h</h2>\n  <p>c</p>\n  <p>b</p>\n</section>"
    );
}

/// The suppression is keyed on the format the target drops, not on a raw block
/// rendering to something short. An `=html` payload still owes its line, so a
/// change that skipped every raw block would be caught here.
#[test]
fn a_matching_raw_block_still_owes_its_line() {
    assert_eq!(
        carve::to_html("a\n\n```=html\n<hr id=\"x\">\n```\n\nb\n"),
        "<p>a</p>\n<hr id=\"x\">\n<p>b</p>"
    );
}

/// Owing no line does not mean going unreported: CARVE-P2-024 requires the
/// drop stay observable, so the block still reaches the loss record.
#[test]
fn a_dropped_raw_block_that_owes_no_line_is_still_reported() {
    let result = carve::to_html_with_report(
        "a\n\n```=latex\n```\n\nb\n",
        carve::CheckedRenderOptions::default(),
    )
    .expect("the render stays inside the ceiling");
    assert_eq!(result.value, "<p>a</p>\n<p>b</p>");
    assert_eq!(result.total_losses, 1);
    assert_eq!(result.losses[0].code, "raw-format-dropped");
    assert_eq!(result.losses[0].format.as_deref(), Some("latex"));
}
