#[test]
fn stacked_list_first_block_quote_stays_in_inner_item() {
    assert_eq!(
        carve::to_html("  - - +\n  > b"),
        "<ul>\n  <li>\n    <ul>\n      <li>+</li>\n    </ul>\n  </li>\n</ul>\n<blockquote><p>b</p></blockquote>"
    );
}

#[test]
fn stacked_task_first_block_quote_stays_in_inner_item() {
    assert_eq!(
        carve::to_html("- - [ ] +\n>\nq"),
        "<ul>\n  <li>\n    <ul class=\"task-list\">\n      <li><input type=\"checkbox\" disabled aria-label=\"+\"> +</li>\n    </ul>\n  </li>\n</ul>\n<blockquote>\n\n</blockquote>\n<p>q</p>"
    );
}
