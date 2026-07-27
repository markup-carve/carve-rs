//! Strict column-0 rule (docs/divergence-from-djot.md §11) for block-attribute
//! lines and image captions: an INDENTED `{attr}` line does not attach to the
//! following block, and an INDENTED image + `^ caption` pair does not form a
//! `<figure>`. Both fold as literal paragraph text, matching carve-php and
//! carve-js. A flush-left (content-column) `{attr}` line or caption MUST still
//! fire -- the column-0 firing is unchanged.
//!
//! Note: once the `{...}` is literal text, its interior `#intro` still renders
//! as a tag span (`<span class="tag">`), exactly as in any other literal text --
//! this is inline rendering and is deliberately untouched.

#[test]
fn indented_attr_line_before_heading_stays_literal() {
    assert_eq!(
        carve::to_html(" {.large #intro}\n # Title\n"),
        "<p>{.large <span class=\"tag\"><strong>#intro</strong></span>}\n# Title</p>"
    );
}

#[test]
fn indented_attr_line_before_paragraph_stays_literal() {
    assert_eq!(
        carve::to_html(" {.note}\n This paragraph.\n"),
        "<p>{.note}\nThis paragraph.</p>"
    );
}

#[test]
fn indented_attr_line_before_list_stays_literal() {
    assert_eq!(
        carve::to_html(" {.todo}\n - one\n - two\n"),
        "<p>{.todo}\n- one\n- two</p>"
    );
}

#[test]
fn indented_attr_line_before_fence_stays_literal() {
    assert_eq!(
        carve::to_html(" {.fancy #x}\n ```php\n code\n ```\n"),
        "<p>{.fancy <span class=\"tag\"><strong>#x</strong></span>}\n<code>php\ncode\n</code></p>"
    );
}

#[test]
fn indented_image_with_caption_stays_literal() {
    assert_eq!(
        carve::to_html(" ![Apollo](a.jpg)\n ^ Figure 1: moon\n"),
        "<p><img src=\"a.jpg\" alt=\"Apollo\">\n^ Figure 1: moon</p>"
    );
}

#[test]
fn indented_attr_then_image_with_caption_stays_literal() {
    assert_eq!(
        carve::to_html(" {.gallery}\n ![Apollo](a.jpg)\n ^ Figure 1: moon\n"),
        "<p>{.gallery}\n<img src=\"a.jpg\" alt=\"Apollo\">\n^ Figure 1: moon</p>"
    );
}

// Regression anchors: flush-left (content-column) constructs MUST keep firing.

#[test]
fn flush_attr_line_before_heading_still_applies() {
    assert_eq!(
        carve::to_html("{.large #intro}\n# Title\n"),
        "<section id=\"intro\">\n  <h1 class=\"large\">Title</h1>\n</section>"
    );
}

#[test]
fn flush_image_with_caption_still_forms_figure() {
    assert_eq!(
        carve::to_html("![Apollo](a.jpg)\n^ Figure 1: moon\n"),
        "<figure>\n  <img src=\"a.jpg\" alt=\"Apollo\">\n  <figcaption>Figure 1: moon</figcaption>\n</figure>"
    );
}

// A flush-left resolved REFERENCE image + caption must still promote to a figure
// (the promote-time path this fix gates on `at_content_column`).
#[test]
fn flush_reference_image_with_caption_still_forms_figure() {
    assert_eq!(
        carve::to_html("![Apollo][a]\n^ Figure 1: moon\n\n[a]: a.jpg\n"),
        "<figure>\n  <img src=\"a.jpg\" alt=\"Apollo\">\n  <figcaption>Figure 1: moon</figcaption>\n</figure>"
    );
}

// An INDENTED `{...}` line in the MIDDLE of an open paragraph must not interrupt
// it (the interrupts_paragraph path, not just the block-start path). It folds as
// literal text, matching carve-php / carve-js / the oracle.
#[test]
fn indented_attr_line_does_not_interrupt_a_paragraph() {
    assert_eq!(
        carve::to_html("Para\n {.x}\nNext\n"),
        "<p>Para\n{.x}\nNext</p>"
    );
}

// A FLUSH-LEFT `{...}` line still interrupts (floats forward as attrs), so it is
// not caught by the guard above.
#[test]
fn flush_attr_line_still_floats_forward() {
    assert_eq!(carve::to_html("{.x}\nPara\n"), "<p class=\"x\">Para</p>");
}
