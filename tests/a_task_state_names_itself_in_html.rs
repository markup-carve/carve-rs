//! PART 10 §11 / carve#1870. All five unchecked spellings render the same box,
//! so the item names the state it was written with.

use carve::{html_to_carve, parse, render_carve, to_html, HtmlImportOptions};

fn fmt(source: &str) -> String {
    render_carve(&parse(source)).expect("writes")
}

#[test]
fn the_item_names_every_extended_state() {
    for state in ['-', '_', '?'] {
        let html = to_html(&format!("- [{state}] a\n"));
        assert!(
            html.contains(&format!("<li data-task-state=\"{state}\">")),
            "{html}"
        );
    }
}

#[test]
fn the_value_is_escaped_like_any_other() {
    assert!(to_html("- [>] a\n").contains("<li data-task-state=\"&gt;\">"));
}

#[test]
fn the_two_states_the_box_tells_apart_carry_nothing() {
    assert!(!to_html("- [ ] a\n").contains("data-task-state"));
    assert!(!to_html("- [x] a\n").contains("data-task-state"));
    assert!(!to_html("- a\n").contains("data-task-state"));
}

#[test]
fn it_leads_the_authored_attributes() {
    assert!(to_html("-{.c} [?] q\n").contains("<li data-task-state=\"?\" class=\"c\">"));
}

#[test]
fn the_state_survives_a_render_and_import_cycle() {
    let source = fmt("- [-] dropped\n- [x] done\n- [ ] open\n");
    let imported =
        html_to_carve(&to_html(&source), &HtmlImportOptions::default()).expect("imports");
    assert_eq!(fmt(&imported.value), source);
}

#[test]
fn a_value_outside_the_enumeration_stays_the_authors_attribute() {
    let imported = html_to_carve(
        "<ul><li data-task-state=\"/\"><input type=\"checkbox\" disabled> odd</li></ul>",
        &HtmlImportOptions::default(),
    )
    .expect("imports");
    assert_eq!(fmt(&imported.value), "-{data-task-state=/} [ ] odd\n");
}

#[test]
fn a_state_the_box_contradicts_does_not_tick_the_box() {
    let imported = html_to_carve(
        "<ul><li data-task-state=\"x\"><input type=\"checkbox\" disabled> a</li></ul>",
        &HtmlImportOptions::default(),
    )
    .expect("imports");
    assert_eq!(fmt(&imported.value), "-{data-task-state=x} [ ] a\n");
}
