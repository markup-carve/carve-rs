fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_matched_fence_at_a_notes_opener_column_ends_the_note() {
    assert_eq!(
        html("[^c]: para\n%%%\nc\n%%%\n```\nx\n"),
        "<pre><code>x </code></pre>"
    );
}

#[test]
fn a_matched_fence_below_a_quoted_notes_column_hides_its_payload() {
    assert_eq!(
        html("> [^c]: para\n>   more\n>  %%%\n>  c\n>  %%%\n```\nx\n"),
        "<blockquote></blockquote><pre><code>x </code></pre>"
    );
}
