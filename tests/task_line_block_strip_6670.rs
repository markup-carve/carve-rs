//! C2-B-027, C2-B-130 (carve#6670): a task box does not move the item's content
//! column, so a line block behind it keeps the spaces past the task mark.
use carve::options::Options;

fn html(input: &str) -> String {
    carve::render(input, &Options::default())
}

#[test]
fn a_line_block_behind_a_task_box_keeps_the_spaces_past_the_task_mark() {
    let out = html("- [ ] ::: |\n      a\n  :::\n");
    assert_eq!(
        out.trim_end(),
        "<ul class=\"task-list\">\n  <li class=\"task\" style=\"list-style: none\"><input type=\"checkbox\" disabled> \n    <div class=\"line-block\">\n      <p>&nbsp;&nbsp;&nbsp;&nbsp;a</p>\n    </div>\n  </li>\n</ul>"
    );
    assert!(html("- [ ] ::: |\n     a\n  :::\n").contains("<p>&nbsp;&nbsp;&nbsp;a</p>"));
    assert!(html("1. [x] ::: |\n      a\n   :::\n").contains("<p>&nbsp;&nbsp;&nbsp;a</p>"));
}

#[test]
fn a_line_block_in_a_div_behind_a_task_box_keeps_them_too() {
    let out = html("- [ ] ::: note\n      ::: |\n      a\n      :::\n  :::\n");
    assert!(out.contains("<p>&nbsp;&nbsp;&nbsp;&nbsp;a</p>"), "{out}");
}

#[test]
fn a_fence_behind_a_task_box_still_strips_to_its_opener() {
    let out = html("- [ ] ```\n      x\n  ```\n");
    assert!(out.contains("<code>x\n</code>"), "{out}");
}

#[test]
fn without_a_task_box_the_line_block_strips_to_the_content_column() {
    let out = html("- ::: |\n  a\n  :::\n");
    assert!(out.contains("<p>a</p>"), "{out}");
}
