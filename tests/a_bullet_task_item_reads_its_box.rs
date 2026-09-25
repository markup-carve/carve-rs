//! A bullet task item reads a box, not the definition its label names
//! (carve-rs#1888).
//!
//! markup-carve/carve#2273 ruled that cmark-gfm 0.29.0.gfm.13 wins here: it is
//! the reader the importers answer to (markup-carve/carve#2187), it reads a
//! checkbox for `- [x] done` whether or not `[x]:` is defined, and the
//! definition goes unused. GitHub's `/markdown` endpoint reads a link instead
//! and is not that reader.
//!
//! An unused link reference definition renders nothing, so a writer that omits
//! its line loses no output. What the reference-link reading lost was the box
//! itself, which a reader does see.
//!
//! Every case asserts the rendered HTML beside the Carve: `- [x](/u) done` and
//! `- [x] done` differ by two characters in the source and by a link against a
//! checkbox in the output.

use carve::{markdown_to_carve, to_html};

fn imported(markdown: &str) -> (String, String) {
    let carve = markdown_to_carve(markdown);
    let html = to_html(&carve);
    (carve, html)
}

const BOX: &str = r#"<input type="checkbox" checked disabled"#;

#[test]
fn a_defined_label_does_not_turn_the_box_into_a_link() {
    let (carve, html) = imported("- [x] done\n\n[x]: /u\n");
    assert_eq!(carve, "- [x] done\n");
    assert!(html.contains(BOX), "{html}");
    assert!(!html.contains("href"), "{html}");
}

#[test]
fn a_title_on_the_definition_changes_nothing() {
    let (carve, html) = imported("- [x] done\n\n[x]: /u \"Title\"\n");
    assert_eq!(carve, "- [x] done\n");
    assert!(html.contains(BOX), "{html}");
    assert!(!html.contains("Title"), "{html}");
}

#[test]
fn a_tab_after_the_marker_is_the_boxs_own_separator() {
    let (carve, html) = imported("- [x]\tdone\n\n[x]: /u\n");
    assert_eq!(carve, "- [x] done\n");
    assert!(html.contains(BOX), "{html}");
}

#[test]
fn an_upper_case_label_reads_the_same_box() {
    let (carve, html) = imported("- [X] done\n\n[X]: /u\n");
    assert_eq!(carve, "- [x] done\n");
    assert!(html.contains(BOX), "{html}");
}

#[test]
fn a_bullet_task_item_with_no_definition_still_reads_a_box() {
    // THE CONTROL ON THE OTHER SIDE. A fix that simply stopped reading task
    // markers would pass every assertion above and fail this one.
    let (carve, html) = imported("- [x] done\n");
    assert_eq!(carve, "- [x] done\n");
    assert!(html.contains(BOX), "{html}");
}

#[test]
fn a_bullet_task_list_nested_in_an_ordered_item_keeps_its_boxes() {
    let (carve, html) = imported("1. outer\n\n   - [x] inner\n\n[x]: /u\n");
    assert!(carve.contains("- [x] inner"), "{carve}");
    assert!(html.contains(BOX), "{html}");
}

#[test]
fn an_ordered_task_item_still_keeps_its_marker_as_text() {
    // carve-rs#1886's reading, which is the same ruling spelled where the
    // writer has no box to put.
    let (carve, html) = imported("1. [x] done\n\n[x]: /u\n");
    assert_eq!(carve, "1. [x] done\n");
    assert!(html.contains("<li>[x] done</li>"), "{html}");
}

#[test]
fn a_shortcut_reference_link_that_is_not_a_task_marker_still_reads_as_a_link() {
    // The removed branch read a LINK where a task marker sat. Nothing about
    // ordinary shortcut references changes, and without this the fix could have
    // taken the reference reading away from every label.
    let (carve, html) = imported("- see [x] there\n\n[x]: /u\n");
    assert_eq!(carve, "- see [x](/u) there\n");
    assert!(html.contains(r#"<a href="/u">x</a>"#), "{html}");
    assert!(!html.contains("checkbox"), "{html}");
}
