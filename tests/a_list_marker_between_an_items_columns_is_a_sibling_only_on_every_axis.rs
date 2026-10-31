//! PART 0 S4 with §11 N1 (carve#4545): between an item's marker and content
//! columns a list marker is a sibling only when it matches the open list on
//! every axis; any other marker there is text of the open paragraph.

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_task_marker_under_a_plain_item_folds() {
    assert_eq!(html("- a\n - [ ] b\n"), "<ul> <li>a - [ ] b</li> </ul>");
    assert_eq!(
        html("- [ ] a\n - b\n"),
        "<ul class=\"task-list\"> <li><input type=\"checkbox\" disabled aria-label=\"a - b\"> a - b</li> </ul>"
    );
}

#[test]
fn an_ordered_marker_of_another_dialect_folds() {
    assert_eq!(html("1. a\n a. b\n"), "<ol> <li>a a. b</li> </ol>");
    assert_eq!(html("1. a\n i. b\n"), "<ol> <li>a i. b</li> </ol>");
    assert_eq!(
        html("a. a\n 1. b\n"),
        "<ol type=\"a\"> <li>a 1. b</li> </ol>"
    );
}

#[test]
fn a_nested_item_folds_the_same_way() {
    assert_eq!(
        html("- x\n  - a\n   - [ ] b\n"),
        "<ul> <li>x <ul> <li>a - [ ] b</li> </ul> </li> </ul>"
    );
}

#[test]
fn a_full_sibling_still_opens_an_item() {
    assert_eq!(
        html("- [ ] a\n - [x] b\n"),
        "<ul class=\"task-list\"> <li><input type=\"checkbox\" disabled aria-label=\"a\"> a</li> <li data-task-state=\"x\"><input type=\"checkbox\" checked disabled aria-label=\"b\"> b</li> </ul>"
    );
    assert_eq!(
        html("i. a\n ii. b\n"),
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
