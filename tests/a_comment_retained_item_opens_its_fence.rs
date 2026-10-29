fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_comment_retained_item_opens_an_unclosed_code_fence() {
    for comment in ["%% c\n", " %% c\n", " %%%\n %%%\n"] {
        assert_eq!(
            html(&format!("- para\n{comment}```\n")),
            "<ul><li>para</li></ul><pre><code></code></pre>"
        );
    }
}

#[test]
fn a_matched_comment_with_a_body_retains_the_item_fence() {
    assert_eq!(
        html("- para\n %%%\n c\n %%%\n```\nx\n"),
        "<ul><li>para</li></ul><pre><code>x </code></pre>"
    );
}

#[test]
fn a_quoted_item_retains_an_empty_fence_after_a_matched_comment() {
    assert_eq!(
        html("- > para\n  > more\n %%%\n c\n %%%\n```\nx\n"),
        "<ul><li><blockquote><p>para more</p></blockquote></li></ul><pre><code>x </code></pre>"
    );
}

#[test]
fn a_quoted_item_without_a_marked_continuation_keeps_the_comment() {
    assert_eq!(
        html("- > para\n %%%\n c\n %%%\n```\nx\n"),
        "<ul><li><blockquote><p>para</p></blockquote></li></ul><pre><code>x </code></pre>"
    );
}

#[test]
fn an_ordered_marked_quote_retains_the_empty_fence() {
    assert_eq!(
        html("1. > para\n   > more\n %%%\n c\n %%%\n```\nx\n"),
        "<ol><li><blockquote><p>para more</p></blockquote></li></ol><pre><code>x </code></pre>"
    );
}

#[test]
fn two_empty_comment_spans_keep_the_marked_quote_fence() {
    assert_eq!(
        html("- > para\n  > more\n %%%\n %%%\n %%%\n %%%\n```\nx\n"),
        "<ul><li><blockquote><p>para more</p></blockquote></li></ul><pre><code>x </code></pre>"
    );
}

#[test]
fn a_prior_code_fence_does_not_change_the_comment_pair() {
    assert_eq!(
        html("```\n%%%\n```\n- > para\n  > more\n %%%\n c\n %%%\n```\nx\n"),
        "<pre><code>%%% </code></pre><ul><li><blockquote><p>para more</p></blockquote></li></ul><pre><code>x </code></pre>"
    );
}

#[test]
fn an_ordered_quoted_item_keeps_a_matched_comment_body() {
    assert_eq!(
        html("1. > para\n  %%%\n  c\n  %%%\n```\nx\n"),
        "<ol><li><blockquote><p>para</p></blockquote></li></ol><pre><code>x </code></pre>"
    );
}

#[test]
fn a_quoted_item_clears_its_claim_after_the_fence() {
    assert_eq!(
        html("- > para\n%% c\n```\nx\n"),
        "<ul><li><blockquote><p>para</p></blockquote></li></ul><pre><code>x </code></pre>"
    );
}

#[test]
fn a_retained_fence_blank_keeps_the_item_tight() {
    assert_eq!(
        html("- para\n%% c\n```\n  x\n\n  y\n"),
        "<ul><li>para</li></ul><pre><code> x y </code></pre>"
    );
}

#[test]
fn a_blank_at_the_end_of_retained_code_separates_sibling_items() {
    let source = "- a\n%% c\n```\n  x\n\n- b\n";
    let rendered = carve::to_html(source);
    assert!(
        rendered.contains("<pre><code>  x\n\n- b\n</code></pre>"),
        "{rendered}"
    );
    assert_eq!(
        html(source),
        "<ul><li>a</li></ul><pre><code> x - b </code></pre>"
    );
}
