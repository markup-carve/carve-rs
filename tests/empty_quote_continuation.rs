use carve::to_html;

#[test]
fn a_marker_attaches_a_paragraph_to_an_empty_quote() {
    assert_eq!(
        to_html(">\n+\nmore"),
        "<blockquote><p>more</p></blockquote>"
    );
}

#[test]
fn a_marker_attaches_a_block_to_an_empty_quote() {
    assert_eq!(
        to_html(">\n+\n```\ncode\n```"),
        "<blockquote>\n  <pre><code>code\n</code></pre>\n</blockquote>"
    );
}
