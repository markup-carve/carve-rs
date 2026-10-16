//! A comment or a raw block for another target contributes no slot
//! (CARVE-P10-012), so a quote holding one paragraph beside them keeps the
//! compact `<blockquote><p>` form carve-js, carve-php and the oracle write.
use carve::to_html;

#[test]
fn a_quoted_comment_line_keeps_the_compact_quote() {
    for source in [
        "> para\n> %% c\n",
        "> %% c\n> para\n",
        "> para\n>\n> %% c\n",
        "> para\n> %% c\n> %% d\n",
        "> para\n> {#x}\n> %% c\n",
        "> para\n> %%%\n> block\n> %%%\n",
    ] {
        assert_eq!(
            to_html(source),
            "<blockquote><p>para</p></blockquote>",
            "{source:?}"
        );
    }
}

#[test]
fn a_raw_block_for_another_target_keeps_the_compact_quote() {
    assert_eq!(
        to_html("> para\n> ```=latex\n> x\n> ```\n"),
        "<blockquote><p>para</p></blockquote>"
    );
}

#[test]
fn a_nested_quote_with_a_comment_keeps_the_compact_inner_quote() {
    assert_eq!(
        to_html("> > para\n> > %% c\n"),
        "<blockquote>\n  <blockquote><p>para</p></blockquote>\n</blockquote>"
    );
}

#[test]
fn a_comment_beside_two_paragraphs_keeps_the_expanded_quote() {
    assert_eq!(
        to_html("> para\n> %% c\n> more\n"),
        "<blockquote>\n  <p>para</p>\n  <p>more</p>\n</blockquote>"
    );
}

#[test]
fn a_quoted_comment_before_an_unclosed_fence_in_a_container() {
    assert_eq!(
        to_html("::: note\n> para\n> %% c\n```\ncode\n::::\n:::\nafter *b*\n"),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <blockquote><p>para</p></blockquote>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"
    );
}
