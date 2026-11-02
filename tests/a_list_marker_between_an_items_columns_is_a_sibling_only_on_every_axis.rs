//! PART 0 S4 with §11 N1 (carve#4545, superseding ruling): between an item's
//! marker and content columns a list marker reads as it does at column 0: a
//! sibling if it matches the list on every axis, otherwise a new list.

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_task_marker_under_a_plain_item_opens_a_new_list() {
    assert_eq!(html("- a\n - [ ] b\n"), "<ul> <li>a</li> </ul> <ul class=\"task-list\"> <li><input type=\"checkbox\" disabled aria-label=\"b\"> b</li> </ul>");
    assert_eq!(
        html("- [ ] a\n - b\n"),
        "<ul class=\"task-list\"> <li><input type=\"checkbox\" disabled aria-label=\"a\"> a</li> </ul> <ul> <li>b</li> </ul>"
    );
}

#[test]
fn an_ordered_marker_of_another_dialect_opens_a_new_list() {
    assert_eq!(
        html("a.  a\n 1. b\n"),
        "<ol type=\"a\"> <li>a</li> </ol> <ol> <li>b</li> </ol>"
    );
    // One space after a letter or a roman numeral with `.` is no marker (carve#4811).
    assert_eq!(html("1. a\n a. b\n"), "<ol> <li>a a. b</li> </ol>");
    assert_eq!(html("1. a\n i. b\n"), "<ol> <li>a i. b</li> </ol>");
}

#[test]
fn a_nested_item_reads_the_same_way() {
    assert_eq!(
        html("- x\n  - a\n   - [ ] b\n"),
        "<ul> <li>x <ul> <li>a</li> </ul> <ul class=\"task-list\"> <li><input type=\"checkbox\" disabled aria-label=\"b\"> b</li> </ul> </li> </ul>"
    );
}

#[test]
fn a_full_sibling_still_opens_an_item() {
    assert_eq!(
        html("- [ ] a\n - [x] b\n"),
        "<ul class=\"task-list\"> <li><input type=\"checkbox\" disabled aria-label=\"a\"> a</li> <li data-task-state=\"x\"><input type=\"checkbox\" checked disabled aria-label=\"b\"> b</li> </ul>"
    );
    assert_eq!(
        html("i.  a\n ii.  b\n"),
        "<ol type=\"i\"> <li>a</li> <li>b</li> </ol>"
    );
}

#[test]
fn at_the_list_column_a_new_list_opens() {
    assert_eq!(
        html("- a\n- [ ] b\n"),
        "<ul> <li>a</li> </ul> <ul class=\"task-list\"> <li><input type=\"checkbox\" disabled aria-label=\"b\"> b</li> </ul>"
    );
}
