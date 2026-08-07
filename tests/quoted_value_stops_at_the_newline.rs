//! A QUOTED VALUE STOPS AT THE NEWLINE (PART 4, carve#888).
//!
//! `quoted_value` excludes a newline in BOTH of its alternatives. An attribute
//! value in quotes ends at the closing quote on the same line; a line break
//! inside the quotes is not content - it ends the production, and the whole
//! attribute block is unrecognized.
//!
//! Settled the executable spec's way, because the alternative falsifies a
//! sentence the grammar already states. An inline attribute block cannot span
//! lines (carve#897), and since carve#906 its padding takes `space` and its
//! separator `space+`, neither of which admits a break. The quoted value was
//! the last way through.
//!
//! THE BLOCK-ATTRIBUTE LINE IS THE HALF WITH A COST, and the half this engine
//! had wrong. `block_attributes` reads the same `quoted_value`, so a break
//! inside a quoted value ends that block too. This engine COLLAPSED the break
//! to a space - a reading no production in either normative file describes.
//! All three engines accepted the shape and no two agreed on what it meant,
//! which is what an unstated rule looks like, and why this is a defect rather
//! than one of two defensible readings.
//!
//! A block attribute may still span lines: `continuation` is where a newline IS
//! admitted, and it sits BETWEEN two tokens rather than inside one.

use carve::to_html;

// ---------------------------------------------------------------------------
// The rule
// ---------------------------------------------------------------------------

#[test]
fn an_inline_block_ends_at_a_newline_inside_a_quoted_value() {
    assert_eq!(
        to_html("*x*{k=\"a\nb\"}\n").trim(),
        "<p><strong>x</strong>{k=\u{201c}a\nb\u{201d}}</p>"
    );
}

#[test]
fn a_block_attribute_line_ends_at_a_newline_inside_a_quoted_value() {
    // The row this engine had wrong: it accepted the block and collapsed the
    // newline to a space, so `paragraph` came out carrying `k="a b"`.
    assert_eq!(
        to_html("{k=\"a\nb\"}\n\nparagraph\n").trim(),
        "<p>{k=\u{201c}a\nb\u{201d}}</p>\n<p>paragraph</p>"
    );
}

#[test]
fn the_single_quoted_form_answers_the_same_way() {
    assert_eq!(
        to_html("{k='a\nb'}\n\nparagraph\n").trim(),
        "<p>{k=\u{2018}a\nb\u{2019}}</p>\n<p>paragraph</p>"
    );
    assert_eq!(
        to_html("*x*{k='a\nb'}\n").trim(),
        "<p><strong>x</strong>{k=\u{2018}a\nb\u{2019}}</p>"
    );
}

// ---------------------------------------------------------------------------
// CONTROLS
// ---------------------------------------------------------------------------

#[test]
fn control_the_same_value_on_one_line_is_an_ordinary_attribute() {
    // The rule is about the line break, not about the quotes.
    assert_eq!(
        to_html("*x*{k=\"a b\"}\n").trim(),
        "<p><strong k=\"a b\">x</strong></p>"
    );
    assert_eq!(
        to_html("{k=\"a b\"}\n\nparagraph\n").trim(),
        "<p k=\"a b\">paragraph</p>"
    );
}

#[test]
fn control_a_continuation_between_two_attributes_is_still_one_block() {
    // `continuation` admits the newline BETWEEN tokens, never inside one.
    assert_eq!(
        to_html("{.a\n.b}\n\nparagraph\n").trim(),
        "<p class=\"a b\">paragraph</p>"
    );
}

#[test]
fn control_a_continuation_after_a_closed_quoted_value_is_still_one_block() {
    // The value closes on its own line, so the break that follows is a
    // continuation. An escaped quote inside the value must not read as the
    // closer, or this block would be unrecognized.
    assert_eq!(
        to_html("{k=\"a\\\"x\"\n.b}\n\nparagraph\n").trim(),
        "<p k=\"a&quot;x\" class=\"b\">paragraph</p>"
    );
}

#[test]
fn control_an_escaped_quote_outside_a_value_opens_nothing() {
    // A backslash escapes the next character wherever it sits, so `\"` in an
    // UNQUOTED value neither opens a quoted value nor closes one. Reading it as
    // an opener would refuse a block that is valid today, and the executable
    // spec renders this one as an ordinary block.
    assert_eq!(
        to_html("{k=a\\\"x\n.b}\n\nparagraph\n").trim(),
        "<p k=\"a\\&quot;x\" class=\"b\">paragraph</p>"
    );
}

#[test]
fn control_a_value_that_closes_before_the_break_keeps_the_block() {
    // Two values, the first closed on its own line: the break after it is a
    // continuation, and only the SECOND value's break ends the block.
    assert_eq!(
        to_html("{k=\"a\"\n.b}\n\nparagraph\n").trim(),
        "<p k=\"a\" class=\"b\">paragraph</p>"
    );
    assert_eq!(
        to_html("{k=\"a\"\nk2=\"b\nc\"}\n\nparagraph\n").trim(),
        "<p>{k=\u{201c}a\u{201d}\nk2=\u{201c}b\nc\u{201d}}</p>\n<p>paragraph</p>"
    );
}

#[test]
fn control_a_blank_line_still_ends_the_block() {
    assert_eq!(
        to_html("{.a\n\n.b}\n\nparagraph\n").trim(),
        "<p>{.a</p>\n<p>.b}</p>\n<p>paragraph</p>"
    );
}
