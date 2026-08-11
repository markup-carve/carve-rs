//! An `<ol>`'s `type` and `start` are the element's own shape, so they are
//! written BEFORE the author's attributes.
//!
//! This engine wrote them after, reading PART 11 section 5.1's "a generated
//! attribute joins at the end" as covering them. That rule is about an
//! attribute added by processing on top of what the author wrote - its own
//! example is a heading's auto-slug id. `type` and `start` come from the first
//! item's marker instead.
//!
//! carve-js, carve-php and reference djot 0.3.2 all lead with the structural
//! attribute (markup-carve/carve#1090).

#[test]
fn type_precedes_the_authored_attributes() {
    assert_eq!(
        carve::to_html("{k=v .attr}\na. alpha\n"),
        "<ol type=\"a\" k=\"v\" class=\"attr\">\n  <li>alpha</li>\n</ol>",
    );
}

#[test]
fn start_precedes_the_authored_attributes() {
    assert_eq!(
        carve::to_html("{.attr}\n5. five\n"),
        "<ol start=\"5\" class=\"attr\">\n  <li>five</li>\n</ol>",
    );
}

#[test]
fn an_id_keeps_its_authored_position_after_the_structural_one() {
    assert_eq!(
        carve::to_html("{#i .attr}\na. alpha\n"),
        "<ol type=\"a\" id=\"i\" class=\"attr\">\n  <li>alpha</li>\n</ol>",
    );
}

/// BOUND, not proof: a decimal marker emits no `type`, so there is nothing to
/// lead and the output is unchanged. Reverting the reorder leaves this passing,
/// which is why it is here - it fails only if a change disturbs the authored
/// attributes themselves.
#[test]
fn a_decimal_marker_has_no_structural_attribute_to_lead() {
    assert_eq!(
        carve::to_html("{.attr}\n1. one\n"),
        "<ol class=\"attr\">\n  <li>one</li>\n</ol>",
    );
}

/// BOUND: an unordered list has no structural attribute either.
#[test]
fn an_unordered_list_is_unchanged() {
    assert_eq!(
        carve::to_html("{.attr}\n- one\n"),
        "<ul class=\"attr\">\n  <li>one</li>\n</ul>",
    );
}
