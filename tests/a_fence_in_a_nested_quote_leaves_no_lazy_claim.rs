//! A fence in a nested quote leaves no paragraph for a following unmarked line
//! to continue, so that line leaves every quote (markup-carve/carve-rs#2117).
//!
//! PART 1, owner selection: a quote needs its marker or a continuation claim
//! stored by the preceding ORDINARY line, and "a blank, heading, definition,
//! comment, table or closed fence establishes no such claim". A fence opener
//! or a line inside an open fence is not an ordinary line either.
//!
//! The collector used to judge the last nested line on its own, where a fence
//! opener with no closer after it reads as paragraph text.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

/// Both ways of ending a zero-line fence publish an empty payload.
const UNCLOSED_EMPTY_CODE: &str = "<pre><code></code></pre>";
const CLOSED_EMPTY_CODE: &str = "<pre><code></code></pre>";

#[test]
fn an_unterminated_fence_at_the_start_of_a_nested_quote() {
    assert_eq!(
        html("> > ```\nc\n"),
        format!("<blockquote>\n  <blockquote>\n    {UNCLOSED_EMPTY_CODE}\n  </blockquote>\n</blockquote>\n<p>c</p>")
    );
}

#[test]
fn a_nested_quote_opened_under_a_paragraph() {
    assert_eq!(
        html("> a\n> > ```\nc\n"),
        format!(
            "<blockquote>\n  <p>a</p>\n  <blockquote>\n    {UNCLOSED_EMPTY_CODE}\n  </blockquote>\n</blockquote>\n<p>c</p>"
        )
    );
}

#[test]
fn a_line_inside_an_open_nested_fence() {
    assert_eq!(
        html("> > ```\n> > x\nc\n"),
        "<blockquote>\n  <blockquote>\n    <pre><code>x\n</code></pre>\n  </blockquote>\n</blockquote>\n<p>c</p>"
    );
}

#[test]
fn a_closed_fence_at_the_start_of_a_nested_quote() {
    assert_eq!(
        html("> > ```\n> > ```\ny\n"),
        format!("<blockquote>\n  <blockquote>\n    {CLOSED_EMPTY_CODE}\n  </blockquote>\n</blockquote>\n<p>y</p>")
    );
}

#[test]
fn a_fence_closed_one_level_down_after_a_paragraph() {
    // The closer is found at the fence's own depth.
    assert_eq!(
        html("> > a\n> > ```\n> > ```\ny\n"),
        format!(
            "<blockquote>\n  <blockquote>\n    <p>a</p>\n    {CLOSED_EMPTY_CODE}\n  </blockquote>\n</blockquote>\n<p>y</p>"
        )
    );
}

#[test]
fn a_fence_back_at_the_outer_level_opens_after_a_nested_fence() {
    // The nested fence left no paragraph, so the outer fence opens at block
    // start without a closer, and `y` leaves the quote rather than joining it.
    assert_eq!(
        html("> a\n> > ```\n> ```\ny\n"),
        format!(
            "<blockquote>\n  <p>a</p>\n  <blockquote>\n    {UNCLOSED_EMPTY_CODE}\n  </blockquote>\n  {UNCLOSED_EMPTY_CODE}\n</blockquote>\n<p>y</p>"
        )
    );
}

#[test]
fn control_a_fence_with_no_closer_after_a_paragraph_is_inline_text() {
    // §10: after a paragraph a fence interrupts only with a closer ahead, so the
    // paragraph stays open and `c` continues it.
    assert_eq!(
        html("> > a\n> > ```\nc\n"),
        "<blockquote>\n  <blockquote><p>a\n<code>\nc</code></p></blockquote>\n</blockquote>"
    );
}
