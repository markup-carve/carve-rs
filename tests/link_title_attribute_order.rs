//! `href`, then the authored title, then the attribute block.
//!
//! The renderer emitted the attribute block BEFORE the title, so a link
//! carrying both published them in the opposite order from carve-js, carve-php
//! and the executable spec (carve-rs#543). No corpus document pairs the two,
//! which is why nothing compared them.

use carve::to_html;

#[test]
fn an_inline_link_writes_the_title_before_its_attributes() {
    assert_eq!(
        to_html("[E](/u \"T\"){.x}"),
        "<p><a href=\"/u\" title=\"T\" class=\"x\">E</a></p>"
    );
}

#[test]
fn a_reference_link_writes_them_in_the_same_order() {
    assert_eq!(
        to_html("[E][ex]{.x}\n\n[ex]: /u \"T\""),
        "<p><a href=\"/u\" title=\"T\" class=\"x\">E</a></p>"
    );
}

#[test]
fn a_destination_title_occupies_the_html_slot() {
    assert_eq!(
        to_html("[E](/u \"T\"){TITLE=Z}"),
        "<p><a href=\"/u\" title=\"T\">E</a></p>"
    );
}

#[test]
fn an_attribute_title_survives_without_a_destination_title() {
    assert_eq!(
        to_html("[E](/u){title=Z}"),
        "<p><a href=\"/u\" title=\"Z\">E</a></p>"
    );
}

#[test]
fn an_image_was_already_in_that_order() {
    assert_eq!(
        to_html("![A](/i \"T\"){.x}"),
        "<img src=\"/i\" alt=\"A\" title=\"T\" class=\"x\">"
    );
}

#[test]
fn an_image_destination_title_occupies_the_html_slot() {
    assert_eq!(
        to_html("![A](/i \"T\"){title=Z}"),
        "<img src=\"/i\" alt=\"A\" title=\"T\">"
    );
}
