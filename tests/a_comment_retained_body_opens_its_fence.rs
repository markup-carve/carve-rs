fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_column_zero_comment_keeps_the_description_fence_in_its_body() {
    assert_eq!(
        html(":: t\n: para\n%% c\n```\n"),
        "<dl><dt>t</dt><dd>para</dd></dl><pre><code></code></pre>"
    );
}

#[test]
fn a_below_column_comment_keeps_the_description_fence_in_its_body() {
    assert_eq!(
        html(":: t\n: para\n %% c\n```\n"),
        "<dl><dt>t</dt><dd>para</dd></dl><pre><code></code></pre>"
    );
}

#[test]
fn a_column_zero_line_exits_an_unclosed_retained_description_fence() {
    let expected = "<dl><dt>t</dt><dd>para</dd></dl><pre><code>x </code></pre>";
    assert_eq!(html(":: t\n: para\n%% c\n```\nx\n"), expected);
    assert_eq!(html(":: t\n: para\n %% c\n```\nx\n"), expected);
}

#[test]
fn a_column_zero_line_exits_an_unclosed_retained_admonition_fence() {
    assert_eq!(
        html(":: t\n: ::: note\n  para\n%% c\n```\nx\n"),
        "<dl><dt>t</dt><dd><aside class=\"admonition note\" aria-label=\"Note\"><p>para</p></aside></dd></dl><pre><code>x </code></pre>"
    );
}

#[test]
fn a_comment_under_an_item_keeps_the_nested_description_fence_in_its_body() {
    let expected =
        "<ul><li><dl><dt>t</dt><dd><p>para</p><pre><code></code></pre></dd></dl></li></ul>";
    assert_eq!(html("- :: t\n  : para\n%% c\n```\n"), expected);
    assert_eq!(html("- :: t\n  : para\n %% c\n ```\n"), expected);
}

#[test]
fn a_column_zero_comment_keeps_the_footnote_fence_in_its_body() {
    assert_eq!(html("[^c]: para\n%% c\n```\n"), "<pre><code></code></pre>");
}

#[test]
fn a_below_column_comment_keeps_the_footnote_fence_in_its_body() {
    assert_eq!(html("[^c]: para\n %% c\n```\n"), "<pre><code></code></pre>");
}

#[test]
fn a_comment_on_a_footnote_term_preserves_the_terms_trailing_space() {
    let source = "[r][] [^p] [^c] [^d]\n[^c]: :: t\n %% c\n";
    assert_eq!(html(source), "<p>[r][] [^p] <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> [^d]</p><section role=\"doc-endnotes\" aria-label=\"Footnotes\"><hr><ol><li id=\"fn1\"><dl><dt>t</dt></dl><p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p></li></ol></section>");
}

#[test]
fn a_column_zero_comment_keeps_the_footnote_admonition_fence_in_its_body() {
    assert_eq!(
        html("[^c]: ::: note\n      para\n%% c\n```\n"),
        "<pre><code></code></pre>"
    );
}

#[test]
fn a_column_zero_comment_keeps_the_description_admonition_fence_in_its_body() {
    assert_eq!(html(":: t\n: ::: note\n  para\n%% c\n```\n"), "<dl><dt>t</dt><dd><aside class=\"admonition note\" aria-label=\"Note\"><p>para</p></aside></dd></dl><pre><code></code></pre>");
}

#[test]
fn a_comment_keeps_the_footnote_admonition_lazy_body_open_for_a_fence() {
    assert_eq!(
        html("[^c]: ::: note\n      more\n%% c\n```\n"),
        "<pre><code></code></pre>"
    );
}

#[test]
fn a_comment_keeps_the_footnote_list_body_open_for_a_fence() {
    assert_eq!(
        html("[^c]: 1. para\n %% c\n```\n"),
        "<pre><code></code></pre>"
    );
}

#[test]
fn a_matched_fence_after_a_footnote_comment_keeps_its_payload_outside_the_note() {
    let source = "[^c]: para\n%% c\n```\n  x\n- y\n```\nsee [^c]\n";
    let rendered = carve::to_html(source);
    assert!(rendered.starts_with("<pre><code>  x\n- y\n</code></pre>\n<p>see <a "));
    assert!(rendered.contains("<li id=\"fn1\">"));
}

#[test]
fn an_indented_retained_footnote_fence_uses_its_authored_column() {
    let rendered = carve::to_html("[^c]: para\n%% c\n  ```\n  x\n  ```\nsee [^c]\n");
    assert!(rendered.contains("<pre><code>x\n</code></pre>"));
    assert!(rendered.starts_with("<pre><code>x\n</code></pre>\n<p>see <a "));
}

#[test]
fn a_retained_description_fence_keeps_a_marker_as_code() {
    let source = ":: t\n: para\n%% c\n```\n- y\n```\n";
    let rendered = carve::to_html(source);
    assert!(rendered.contains("<dd>para</dd>\n</dl>\n<pre><code>- y\n</code></pre>"));
}

#[test]
fn an_indented_fence_after_a_retained_description_comment_opens_as_code() {
    let rendered = carve::to_html(":: t\n: para\n %% c\n ```\n  x\n ```\n");
    assert!(rendered.contains("<pre><code> x\n</code></pre>"));
    assert!(rendered.contains("<dd>para</dd>"));
}

#[test]
fn an_unclosed_retained_fence_under_an_item_keeps_a_marker_as_code() {
    let source = "- :: t\n  : para\n %% c\n ```\n - y\n";
    let rendered = carve::to_html(source);
    assert!(rendered.contains("<pre><code>- y\n</code></pre>"));
    assert!(rendered.ends_with("</dd>\n    </dl>\n  </li>\n</ul>"));
}

#[test]
fn a_column_zero_comment_after_a_footnote_term_keeps_its_lazy_paragraph() {
    let rendered = html("[^c]: :: t\n%% c\nNext para\nsee [^c]\n");
    assert!(
        rendered.starts_with("<p>Next para see <a id=\"fnref1\""),
        "{rendered}"
    );
    assert!(
        !rendered.contains("<dl><dt>t</dt></dl><p>Next para"),
        "{rendered}"
    );
}

#[test]
fn a_caption_after_a_retained_description_comment_stays_in_the_body() {
    assert_eq!(
        html(":: t\n: para\n%% c\n^ cap\n"),
        "<dl><dt>t</dt><dd>para</dd></dl><p>^ cap</p>"
    );
}

#[test]
fn an_uncommented_fence_after_a_nested_description_stays_outside_the_item() {
    assert_eq!(
        html("- :: t\n  : para\n```\nx\n```\n"),
        "<ul><li><dl><dt>t</dt><dd>para</dd></dl></li></ul><pre><code>x </code></pre>"
    );
}
