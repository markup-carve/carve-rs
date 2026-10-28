use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim().to_owned()
}

#[test]
fn nested_task_without_a_paragraph_releases_the_outer_item_band() {
    let source = "- -{#k} [x] {#h}\n # h\n";
    assert_eq!(
        html(source),
        "<ul>\n  <li>\n    <ul class=\"task-list\">\n      <li data-task-state=\"x\" id=\"k\"><input type=\"checkbox\" checked disabled> </li>\n    </ul>\n  </li>\n</ul>\n<p># h</p>"
    );
}

#[test]
fn blank_and_comment_close_the_nested_and_outer_lazy_claims() {
    let source = "- - a\n\n  %%\nx\n";
    assert_eq!(
        html(source),
        "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"
    );
}

#[test]
fn a_closed_attached_fence_releases_a_line_below_the_content_column() {
    let source = "- a\n+\n```\n```\n x\n";
    assert_eq!(
        html(source),
        "<ul>\n  <li>a\n    <pre><code></code></pre>\n  </li>\n</ul>\n<p>x</p>"
    );
}
