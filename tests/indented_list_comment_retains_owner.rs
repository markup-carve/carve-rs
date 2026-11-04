use carve::to_html;

#[test]
fn line_comments_at_an_indented_marker_column_keep_the_item() {
    let source = "::: note\n  - x\n  %% c\n  y\n:::\n";
    let html = to_html(source);
    assert!(html.contains("<li>x\ny</li>"), "{source:?}: {html}");
    assert!(html.contains("<li>x\n"), "{source:?}: {html}");
    assert!(!html.contains("<p>y</p>"), "{source:?}: {html}");
    // carve#4907: an unclosed `%%%` opens a span that hides `y`.
    for source in [
        "::: note\n  - x\n  %%%\n  y\n:::\n",
        "  - x\n  %%%\n  y\n",
        "::: note\n   1. x\n   %%%\n   y\n:::\n",
    ] {
        let html = to_html(source);
        assert!(html.contains("<li>x</li>"), "{source:?}: {html}");
        assert!(!html.contains('y'), "{source:?}: {html}");
    }
}

#[test]
fn a_matched_comment_fence_keeps_its_block_boundary() {
    let html = to_html("  - x\n  %%%\n  hidden\n  %%%\n  y\n");
    assert!(!html.contains("hidden"), "{html}");
    assert!(html.contains("<p>y</p>"), "{html}");
}
