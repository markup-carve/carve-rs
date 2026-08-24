//! An HTML import reads a task item's checkbox, so the item comes back a task.
//!
//! markup-carve/carve-rs#1365. `html_import.rs` built every `ListItem` with
//! `checked: None` and nothing in it looked at an `<input>` or at a
//! `type="checkbox"`, so the state was never READ rather than read and dropped.
//! A checked item imported as an ordinary bullet, and rendering the import back
//! gave `<li>a</li>` - the checkbox gone from the source AND from the output, a
//! loss on the round trip rather than a spelling difference.
//!
//! ## Why it hid
//!
//! It was found while porting markup-carve/carve-js#1455
//! (markup-carve/carve-rs#1362), whose own repro could not be run through this
//! engine's importer at all: the shape that ticket turns on is a task item's
//! continuation column, and no task item ever survived the import to have one.
//! A defect that makes another defect unreproducible is why this one sat behind
//! a fix to the writer for the same construct.
//!
//! ## The predicate, and the two engines it is ported from
//!
//! A DIRECT child `<input>` whose `type` is `checkbox`. carve-js's
//! `htmlToCarve` and carve-php's `getDirectCheckboxInput` both spell it that
//! way, and this engine's own renderer writes exactly that shape, so the
//! round trip is closed. `checked` is a boolean attribute, so its PRESENCE is
//! the value - `<input type="checkbox" checked>` and `<input type="checkbox"
//! checked="">` are the same item.
//!
//! The checkbox is CONSUMED into the marker rather than walked as content, so a
//! recognized one leaves no `element-unwrapped` and no `attribute-dropped`
//! behind it. An `<input>` the predicate does NOT reach - one wrapped in a
//! `<p>`, one that is not a checkbox - keeps every diagnostic it had, because
//! nothing about it survived.

use carve::{html_to_carve, to_html, HtmlImportOptions};

fn imported(html: &str) -> carve::HtmlImportResult<String> {
    html_to_carve(html, &HtmlImportOptions::default()).expect("import")
}

fn migrated(html: &str) -> String {
    imported(html).value
}

#[test]
fn a_task_item_imports_as_a_task_item() {
    // The ticket's own repro, asserted on BOTH sides: the source the importer
    // writes, and the render of that source. Before the fix the source was
    // `- a\n- b\n` and the render `<li>a</li>`, so the state was gone twice.
    let carve = migrated(
        "<ul>\n  <li><input type=\"checkbox\" checked disabled> a</li>\n  <li><input type=\"checkbox\" disabled> b</li>\n</ul>",
    );
    assert_eq!(carve, "- [x] a\n- [ ] b\n");
    assert_eq!(
        to_html(&carve),
        "<ul>\n  <li><input type=\"checkbox\" checked disabled aria-label=\"a\"> a</li>\n  <li><input type=\"checkbox\" disabled aria-label=\"b\"> b</li>\n</ul>"
    );
}

#[test]
fn this_engines_own_rendered_task_list_survives_a_round_trip() {
    // The render carries an `aria-label` this engine DERIVED from the item's
    // text (markup-carve/carve-rs#1209 rules that a derived accessible name is
    // not baked back into source). Importing it must therefore give the SAME
    // source it came from, not one carrying the label.
    let source = "- [x] a\n- [ ] b\n";
    let rendered = to_html(source);
    assert_eq!(
        rendered,
        "<ul>\n  <li><input type=\"checkbox\" checked disabled aria-label=\"a\"> a</li>\n  <li><input type=\"checkbox\" disabled aria-label=\"b\"> b</li>\n</ul>"
    );
    assert_eq!(migrated(&rendered), source);
}

#[test]
fn a_recognized_checkbox_is_consumed_rather_than_reported_lost() {
    // It is not walked as content, so it leaves nothing behind: no
    // `element-unwrapped` for the `<input>` and no `attribute-dropped` for the
    // `type`, `checked`, `disabled` or `aria-label` it carried. Before the fix
    // this shape produced five diagnostics naming attributes that were, in
    // fact, lost.
    let result =
        imported("<ul><li><input type=\"checkbox\" checked disabled aria-label=\"a\"> a</li></ul>");
    assert_eq!(result.value, "- [x] a\n");
    assert_eq!(
        result.report.diagnostics.len(),
        0,
        "diagnostics: {:?}",
        result.report.diagnostics
    );
}

#[test]
fn a_boolean_checked_is_read_by_presence_not_by_value() {
    for html in [
        "<ul><li><input type=\"checkbox\" checked> a</li></ul>",
        "<ul><li><input type=\"checkbox\" checked=\"\"> a</li></ul>",
        "<ul><li><input type=\"checkbox\" checked=\"checked\"> a</li></ul>",
    ] {
        assert_eq!(migrated(html), "- [x] a\n", "input: {html}");
    }
}

#[test]
fn an_input_the_predicate_does_not_reach_is_not_a_checkbox() {
    // THE CONTROLS. Each still loses its `<input>`, and each must still SAY so
    // - a predicate wide enough to swallow these would report a task item the
    // source never wrote.
    for (html, carve) in [
        // Not a checkbox.
        (
            "<ul><li><input type=\"text\" value=\"v\"> a</li></ul>",
            "- a\n",
        ),
        // No `type` at all.
        ("<ul><li><input> a</li></ul>", "- a\n"),
        // Not a DIRECT child: a `<p>` wrapper is a paragraph the source spelled,
        // so the item is loose and the `<input>` inside it is ordinary content.
        // carve-js and carve-php draw the line in the same place.
        (
            "<ul><li><p><input type=\"checkbox\" checked disabled> a</p></li></ul>",
            "{loose}\n- a\n",
        ),
    ] {
        let result = imported(html);
        assert_eq!(result.value, carve, "input: {html}");
        assert!(
            result
                .report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("<input>")),
            "an unrecognized <input> still reports its loss; input: {html}, diagnostics: {:?}",
            result.report.diagnostics
        );
    }
}

#[test]
fn an_ordinary_bullet_stays_an_ordinary_bullet() {
    // `checked` stays `None` for an item with no checkbox, so nothing gains a
    // `[ ]` marker it never had.
    let carve = migrated("<ul><li>a</li><li>b</li></ul>");
    assert_eq!(carve, "- a\n- b\n");
    assert_eq!(to_html(&carve), "<ul>\n  <li>a</li>\n  <li>b</li>\n</ul>");
}

#[test]
fn a_consumed_checkbox_does_not_renumber_the_siblings_after_it() {
    // PART 12 §16 / markup-carve/carve#1554: a child lifted out of the walk
    // leaves the ones after it at the index they have in the DOCUMENT, not the
    // one they take in the filtered array. The `<span>` is the li's THIRD child
    // node (input, text, span), so its diagnostic path is `/span[3]` - the
    // checkbox does not pull it back to `/span[2]`. The `<span>` itself
    // unwraps to bare text - its only attribute is a `style`, which this
    // importer does not map - so the diagnostic is where the assertion looks.
    let result =
        imported("<ul><li><input type=\"checkbox\"> a<span style=\"color:red\">b</span></li></ul>");
    assert_eq!(result.value, "- [ ] ab\n");
    let paths: Vec<&str> = result
        .report
        .diagnostics
        .iter()
        .filter_map(|d| d.path.as_deref())
        .collect();
    assert!(paths.contains(&"/ul[1]/li[1]/span[3]"), "paths: {paths:?}");
}

#[test]
fn a_task_item_does_not_loosen_its_list() {
    // The checkbox is consumed into the marker, so it is not a `<p>` and does
    // not vote on tightness (markup-carve/carve-js#1110's rule, unchanged).
    let carve = migrated("<ul><li><input type=\"checkbox\" checked> a</li><li>b</li></ul>");
    assert_eq!(carve, "- [x] a\n- b\n");
}

#[test]
fn what_the_marker_cannot_carry_is_still_reported() {
    // The marker holds ONE thing - whether the box is ticked - so anything else
    // the element carried is lost, and the walk that used to report it no
    // longer reaches the node. Silence there would be a worse report than the
    // one this ticket started from: the loss would be real and unstated.
    let result = imported(
        "<ul><li><input type=\"checkbox\" id=\"x\" class=\"c\" name=\"n\" onclick=\"evil()\"> a</li></ul>",
    );
    assert_eq!(result.value, "- [ ] a\n");
    let messages: Vec<&str> = result
        .report
        .diagnostics
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        messages.contains(&"Dropped event-handler attribute onclick on <input>"),
        "the dangerous-name filter still runs on a consumed checkbox; got {messages:?}"
    );
    assert!(
        messages.contains(
            &"Dropped id, class, name on <input>: a task item's checkbox has no attribute slot"
        ),
        "messages: {messages:?}"
    );
}
