//! PART 0 S4: a nested item measures a later marker against its own column.

use carve::to_html;

#[test]
fn a_mixed_task_marker_keeps_its_child() {
    assert_eq!(
        to_html("- [ ] - b\n  * [ ] c\n    - [ ] d\n").trim_end(),
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <ul>\n      <li>b</li>\n    </ul>\n    <ul class=\"task-list\">\n      <li><input type=\"checkbox\" disabled aria-label=\"c\"> c\n        <ul class=\"task-list\">\n          <li><input type=\"checkbox\" disabled aria-label=\"d\"> d</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}
