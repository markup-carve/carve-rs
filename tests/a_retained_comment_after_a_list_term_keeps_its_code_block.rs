fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_code_fence_after_a_matched_comment_fence_is_a_list_item_block() {
    let source = "1. :: t\n  %%%\n  c\n  %%%\n```\n";
    assert_eq!(
        html(source),
        "<ol><li><dl><dt>t</dt></dl></li></ol><pre><code></code></pre>"
    );
}
