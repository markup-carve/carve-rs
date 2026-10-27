#[test]
fn stacked_list_first_block_quote_stays_in_inner_item() {
    assert_eq!(
        carve::to_html("  - - +\n  > b"),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <blockquote><p>b</p></blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn stacked_task_first_block_quote_stays_in_inner_item() {
    assert_eq!(
        carve::to_html("- - [ ] +\n>\nq"),
        "<ul>\n  <li>\n    <ul class=\"task-list\">\n      <li><input type=\"checkbox\" disabled> \n        <blockquote>\n\n        </blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>q</p>"
    );
}
