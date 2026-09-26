//! A structural class is written after the author's attributes and BEFORE an
//! attribute the engine minted on top of them.
//!
//! PART 10 §1's discriminator has three ranks, and this engine collapsed the
//! last two: with an attribute line carrying no class, the class slot was never
//! recorded, so it fell out at the very end - after the `aria-labelledby` the
//! renderer minted for the marker's own title. carve-js and carve-php both put
//! the class ahead of that name (markup-carve/carve-rs#1970).
//!
//! Where the class goes relative to the AUTHORED attributes is the ruling on
//! markup-carve/carve#2328: after all of them. So `{#t}` keeps its id in front
//! and the class follows it, which is what the rows below pin.

use carve::extensions::TocPlacement;
use carve::{to_html_with_options, Options};

fn nav_opener(source: &str) -> String {
    let toc = TocPlacement::new();
    let mut options = Options::new();
    options.extensions.push(&toc);
    let html = to_html_with_options(source, &options);
    html.split('\n')
        .find(|line| line.contains("<nav"))
        .unwrap_or("NO NAV OPENER")
        .trim_start()
        .to_string()
}

#[test]
fn a_window_attribute_line_keeps_the_class_ahead_of_the_minted_name() {
    assert_eq!(
        nav_opener("{from=2 to=2}\n::: toc \"C\" [L]\n:::\n\n# One\n"),
        "<nav class=\"toc\" aria-labelledby=\"adm-1\">",
    );
}

#[test]
fn an_authored_id_leads_the_class_and_the_class_leads_the_minted_name() {
    assert_eq!(
        nav_opener("{#t}\n::: toc \"C\"\n:::\n\n# One\n"),
        "<nav id=\"t\" class=\"toc\" aria-labelledby=\"adm-1\">",
    );
}

#[test]
fn the_minted_label_of_a_titleless_marker_follows_the_class_too() {
    assert_eq!(
        nav_opener("{#t}\n::: toc\n:::\n\n# One\n"),
        "<nav id=\"t\" class=\"toc\" aria-label=\"Table of contents\">",
    );
}

#[test]
fn every_authored_key_precedes_the_class() {
    assert_eq!(
        nav_opener("{role=navigation}\n::: toc \"C\"\n:::\n\n# One\n"),
        "<nav role=\"navigation\" class=\"toc\" aria-labelledby=\"adm-1\">",
    );
}

/// BOUND: with no attribute line there is nothing authored to follow, so the
/// class already led the minted name and this row does not move.
#[test]
fn a_bare_marker_is_unchanged() {
    assert_eq!(
        nav_opener("::: toc \"C\"\n:::\n\n# One\n"),
        "<nav class=\"toc\" aria-labelledby=\"adm-1\">",
    );
}

/// BOUND: an authored class gives the base class a slot to merge into, so the
/// author's position governs and the following key stays where it was written.
#[test]
fn an_authored_class_slot_still_governs_its_own_position() {
    assert_eq!(
        nav_opener("{k=v .x}\n::: toc \"C\"\n:::\n\n# One\n"),
        "<nav k=\"v\" class=\"toc x\" aria-labelledby=\"adm-1\">",
    );
}

/// BOUND: an authored accessible name suppresses the minting entirely, so there
/// is no engine attribute to rank and the class stays last - which is where the
/// carve#2328 ruling puts it when everything before it is authored.
#[test]
fn an_authored_name_mints_nothing_to_rank() {
    assert_eq!(
        nav_opener("{aria-label=\"A\"}\n::: toc \"C\"\n:::\n\n# One\n"),
        "<nav aria-label=\"A\" class=\"toc\">",
    );
}
