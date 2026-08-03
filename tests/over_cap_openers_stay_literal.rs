//! PART 9 §25 (RESOURCE BOUNDS): an implementation MUST enforce a finite
//! nesting-depth cap "beyond which further openers degrade to LITERAL TEXT
//! rather than recursing", and "past the cap every container kind FLATTENS the
//! same way -- the opener becomes literal paragraph text -- rather than
//! crashing".
//!
//! carve-rs reached the cap and then DROPPED the openers past it. The output
//! for 205 openers and for 8000 was byte-identical, so the amount discarded was
//! invisible: no text, no marker, no diagnostic, exit 0 (carve-rs#418).
//!
//! carve-php keeps them, which is what the rule asks for and what makes the
//! two engines agree.

fn openers(count: usize) -> String {
    let mut source = "::: note\n".repeat(count);
    source.push_str("x\n");
    source
}

#[test]
fn openers_past_the_cap_render_as_literal_text() {
    // 203 openers: 200 nest, 3 are past the cap and become paragraph text
    // alongside the body, in the innermost container.
    let html = carve::to_html(&openers(203));
    assert_eq!(
        html.matches("<aside").count(),
        200,
        "the cap itself still holds"
    );
    assert_eq!(
        html.matches("::: note").count(),
        3,
        "each opener past the cap is literal text, got: {}",
        &html[html.len().saturating_sub(200)..]
    );
}

#[test]
fn the_discarded_amount_is_not_invisible() {
    // The defect that hid this: output identical whether 5 openers were past
    // the cap or 7800. A length that does not move with the input is the shape
    // of silent truncation, so it is asserted directly rather than inferred
    // from the count above.
    let small = carve::to_html(&openers(205));
    let large = carve::to_html(&openers(400));
    assert!(
        large.len() > small.len(),
        "output did not grow with 195 more openers ({} vs {} bytes)",
        large.len(),
        small.len()
    );
}

#[test]
fn the_body_after_the_over_cap_openers_survives() {
    let html = carve::to_html(&openers(205));
    assert!(
        html.contains("x"),
        "the body line must not be lost with the openers"
    );
}

#[test]
fn an_input_at_the_cap_is_unaffected() {
    // Guard against fixing the over-cap path by changing the ordinary one.
    let html = carve::to_html(&openers(200));
    assert_eq!(html.matches("<aside").count(), 200);
    assert_eq!(
        html.matches("::: note").count(),
        0,
        "nothing is past the cap here, so nothing degrades"
    );
}
