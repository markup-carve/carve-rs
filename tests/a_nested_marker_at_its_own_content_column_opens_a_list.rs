//! PART 0 S4: a marker at a missed list's column opens its own list.

use carve::to_html;

#[test]
fn a_task_marker_below_a_marker_line_child_nests_in_its_item() {
    assert_eq!(
        to_html("- [ ] - b\n  - [ ] c\n    - [ ] d\n").trim_end(),
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <ul>\n      <li>b</li>\n    </ul>\n    <ul class=\"task-list\">\n      <li><input type=\"checkbox\" disabled aria-label=\"c\"> c\n        <ul class=\"task-list\">\n          <li><input type=\"checkbox\" disabled aria-label=\"d\"> d</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}
