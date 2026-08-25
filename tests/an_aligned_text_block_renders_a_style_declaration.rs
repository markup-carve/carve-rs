//! `{align=left|right|center}` on an element whose `align` means TEXT
//! ALIGNMENT renders the CSS declaration instead of the deprecated
//! presentational attribute (markup-carve/carve#1755).
//!
//! A CELL's alignment already rendered the declaration - `|> a |` gives
//! `<td style="text-align: right;">` since markup-carve/carve#1741 - so off a
//! cell the same authored concept rendered the other, deprecated way, and
//! `html -> carve -> html` was not a fixed point for it.
//!
//! `<table>` IS SCOPED OUT AND THAT EXCLUSION IS PINNED HERE. On a table
//! `align` is PLACEMENT - the table floats left or right, or centres as a
//! block - which does not map to `text-align` at all, so rewriting it would
//! silently right-align the CELL TEXT of every floated table in every existing
//! document. The same reasoning keeps `img` out.
//!
//! The RAW PASS-THROUGH is pinned too: `align` joins the known-key set
//! (`loose`, `#id`, `.class`), and every other key stays untouched, so the
//! change cannot widen into "rewrite attributes we recognize".

use carve::{html_to_carve, to_html, HtmlImportMode, HtmlImportOptions};

fn imported(html: &str) -> String {
    html_to_carve(
        html,
        &HtmlImportOptions {
            mode: HtmlImportMode::Roundtrip,
            ..Default::default()
        },
    )
    .expect("import")
    .value
}

/// THE CANARY. Every other assertion here is about the renderer, and a stale
/// build serves a renderer from before the edit while reporting a pass. This
/// one is the cheapest thing in the file that CANNOT hold unless the binary
/// linked the source this test shipped with, so a wrong-artifact run fails
/// here first and names itself instead of looking like a behavior bug.
#[test]
fn the_declaration_is_present_in_the_binary_under_test() {
    assert_eq!(
        to_html("{align=right}\npara\n"),
        "<p style=\"text-align: right;\">para</p>",
        "a stale artifact: this binary's renderer predates the text-alignment ruling"
    );
}

#[test]
fn every_text_block_renders_the_declaration() {
    assert_eq!(
        to_html("{align=right}\npara\n"),
        "<p style=\"text-align: right;\">para</p>"
    );
    assert!(to_html("{align=left}\n# H\n").contains("<h1 style=\"text-align: left;\">H</h1>"));
    assert!(to_html("{align=center}\n::: box\nx\n:::\n")
        .contains("<div class=\"box\" style=\"text-align: center;\">"));
}

#[test]
fn every_ruled_value_renders_its_declaration() {
    for value in ["left", "right", "center"] {
        assert_eq!(
            to_html(&format!("{{align={value}}}\npara\n")),
            format!("<p style=\"text-align: {value};\">para</p>")
        );
    }
}

#[test]
fn the_deprecated_attribute_is_gone_where_the_declaration_belongs() {
    assert!(!to_html("{align=right}\npara\n").contains("align=\"right\""));
}

#[test]
fn an_author_style_keeps_one_attribute_with_the_declaration_appended() {
    assert_eq!(
        to_html("{align=right style=\"color: red\"}\npara\n"),
        "<p style=\"color: red; text-align: right;\">para</p>"
    );
}

/// ON A TABLE `align` IS PLACEMENT, NOT TEXT ALIGNMENT. Rewriting it would
/// silently right-align the CELL TEXT of a floated table instead of floating
/// it, so the table keeps the legacy attribute. Do not "tidy" this away.
#[test]
fn a_table_keeps_the_placement_attribute() {
    assert!(to_html("{align=right}\n| a |\n").contains("<table align=\"right\">"));
}

/// The same reason: HTML maps `align` on an image to a float, never to
/// `text-align`.
#[test]
fn an_image_keeps_the_placement_attribute() {
    assert!(to_html("{align=right}\n![alt](x.png)\n").contains("align=\"right\""));
}

#[test]
fn the_raw_pass_through_is_untouched_for_every_other_key() {
    assert_eq!(
        to_html("{banana=yellow}\npara\n"),
        "<p banana=\"yellow\">para</p>"
    );
}

/// markup-carve/carve#1756 ruled `{valign=…}` working as designed.
#[test]
fn valign_off_a_cell_is_unchanged() {
    assert_eq!(
        to_html("{valign=top}\npara\n"),
        "<p valign=\"top\">para</p>"
    );
}

/// Only the three values HTML gives a `text-align` meaning are rewritten.
#[test]
fn a_value_outside_the_ruled_set_passes_through_raw() {
    assert_eq!(
        to_html("{align=justify}\npara\n"),
        "<p align=\"justify\">para</p>"
    );
}

#[test]
fn a_cell_alignment_marker_still_renders_its_own_declaration() {
    assert!(to_html("|> a | b |\n").contains("<td style=\"text-align: right;\">a</td>"));
}

#[test]
fn html_to_carve_to_html_is_a_fixed_point_for_an_aligned_paragraph() {
    let source = "<p style=\"text-align: right;\">x</p>";
    let once = to_html(&imported(source));
    assert_eq!(once, source);
    assert_eq!(to_html(&imported(&once)), once, "the second render drifted");
}
