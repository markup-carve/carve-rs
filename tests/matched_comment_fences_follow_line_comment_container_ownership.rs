use carve::{to_html, to_html_with_options, Options};

fn compact(html: &str) -> String {
    html.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

fn assert_render(source: &str, expected: &str) {
    let html = to_html(source);
    assert_eq!(compact(&html), compact(expected), "{source:?}");
    assert_eq!(
        compact(&to_html_with_options(
            source,
            &Options::default().with_positions(true)
        )),
        compact(expected),
        "position path: {source:?}"
    );
}

#[test]
fn a_matched_fence_at_a_list_column_leaves_no_paragraph_for_column_zero() {
    assert_render(
        "1. 1. para\n   %%%\n   %%%\ntail\n",
        "<ol><li><ol><li>para</li></ol></li></ol><p>tail</p>",
    );
    assert_render(
        "- - para\n  %%%\n  %%%\nx\n",
        "<ul><li><ul><li>para</li></ul></li></ul><p>x</p>",
    );
}

#[test]
fn a_matched_fence_below_a_definition_column_keeps_its_follower() {
    assert_render(
        "- :: t\n  : para\n %%%\n %%%\n    tail\n",
        "<ul><li><dl><dt>t</dt><dd><p>para</p><p>tail</p></dd></dl></li></ul>",
    );
    assert_render(
        ":: t\n: para\n %%%\n %%%\n```\n",
        "<dl><dt>t</dt><dd><p>para</p><pre><code></code></pre></dd></dl>",
    );
}

#[test]
fn a_matched_fence_below_a_note_column_keeps_a_nested_quote() {
    assert_render(
        "[r][] [^p] [^c] [^d]\n[^c]: > para\n  %%%\n  %%%\n      > tail\n",
        "<p>[r][] [^p] <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> [^d]</p><section role=\"doc-endnotes\" aria-label=\"Footnotes\"><hr><ol><li id=\"fn1\"><blockquote><p>para</p></blockquote><blockquote><p>tail</p></blockquote><p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p></li></ol></section>",
    );
}

#[test]
fn an_unclosed_fence_below_a_colon_body_keeps_the_following_paragraph() {
    assert_render(
        "1. ::: note\n   para\n  %%%\ntail\n",
        "<ol><li><aside class=\"admonition note\" aria-label=\"Note\"><p>para</p><p>tail</p></aside></li></ol>",
    );
}
