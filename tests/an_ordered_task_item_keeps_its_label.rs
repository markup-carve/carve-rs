//! An ordered task item keeps the text it was written as (carve-rs#1886).
//!
//! Carve spells a checkbox only behind a bullet, so an ordered item's box has
//! nowhere to go - and the importer used to hand the writer a box it could not
//! spell, which dropped the marker's characters with it: `1. [x] done` came back
//! as `1. done`. That is text loss under either reading of
//! markup-carve/carve#2273: the ruled cmark-gfm reading keeps `[x]`, and the
//! GitHub reading keeps a link whose text is `x`. Neither loses the `x`.
//!
//! Every case asserts the RENDERED output as well as the Carve, because what was
//! wrong is what a reader sees. The bullet form is the control: it already kept
//! its box, so it pins that this fix reaches only the ordered form.

use carve::{markdown_to_carve, to_html};

fn imported(markdown: &str) -> (String, String) {
    let carve = markdown_to_carve(markdown);
    let html = to_html(&carve);
    (carve, html)
}

#[test]
fn an_ordered_task_item_keeps_its_marker_as_text() {
    let (carve, html) = imported("1. [x] done\n");
    assert_eq!(carve, "1. [x] done\n");
    assert!(html.contains("<li>[x] done</li>"), "{html}");
}

#[test]
fn a_bullet_task_item_still_imports_as_a_box() {
    // THE CONTROL. It passes before the fix and after it, so a failure here is
    // the fix having reached a form it was never about.
    let (carve, html) = imported("- [x] done\n");
    assert_eq!(carve, "- [x] done\n");
    assert!(
        html.contains(r#"<input type="checkbox" checked disabled"#),
        "{html}"
    );
    assert!(html.contains("done"), "{html}");
}

#[test]
fn an_unchecked_ordered_task_item_keeps_its_marker_too() {
    let (carve, html) = imported("1. [ ] open\n");
    assert_eq!(carve, "1. [ ] open\n");
    assert!(html.contains("<li>[ ] open</li>"), "{html}");
}

#[test]
fn the_ticket_case_keeps_the_label_and_leaves_the_definition_unused() {
    // markup-carve/carve#2273 ruled that cmark-gfm wins, so the checkbox is what
    // is read here and the link reference definition goes unused - which is why
    // the marker's own characters survive and the definition line does not
    // reappear as a link.
    let (carve, html) = imported("1. [x] done\n\n[x]: /u\n");
    assert_eq!(carve, "1. [x] done\n");
    assert!(html.contains("<li>[x] done</li>"), "{html}");
}

#[test]
fn a_bullet_task_list_nested_in_an_ordered_item_still_gets_its_boxes() {
    // The rule is the list the ITEM belongs to, not the outermost one. Without
    // this a fix that read the outer list would silently take the nested boxes
    // away.
    let (carve, html) = imported("1. outer\n\n   - [x] inner\n");
    assert!(carve.contains("- [x] inner"), "{carve}");
    assert!(
        html.contains(r#"<input type="checkbox" checked disabled"#),
        "{html}"
    );
    assert!(html.contains("inner"), "{html}");
}

#[test]
fn an_ordered_item_that_is_only_a_marker_keeps_it() {
    // No label to carry the text, so the marker is the whole content: the shape
    // where a dropped box leaves an EMPTY item rather than a shorter one.
    let (carve, html) = imported("1. [x]\n");
    assert_eq!(carve, "1. [x]\n");
    assert!(html.contains("<li>[x]</li>"), "{html}");
}
