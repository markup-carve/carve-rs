#[test]
fn an_unclosed_fence_attached_in_a_footnote_ends_at_its_first_blank() {
    let html = carve::to_html("[^1]: +\n```\n\n  tail\n\n[^1]\n");
    assert!(
        html.contains("<pre><code></code></pre>\n      <p>tail<a href=\"#fnref1\""),
        "{html}"
    );
}

#[test]
fn an_unclosed_colon_attached_in_a_footnote_ends_at_its_first_blank() {
    let html = carve::to_html("[^1]: +\n::: note\n\n  tail\n\n[^1]\n");
    assert!(html.contains("<aside class=\"admonition note\" aria-label=\"Note\">\n\n      </aside>\n      <p>tail"), "{html}");
}

#[test]
fn an_unclosed_fence_attached_in_a_description_ends_at_its_first_blank() {
    let html = carve::to_html(":: t\n: +\n```\n\n  tail\n");
    assert_eq!(
        html,
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code></code></pre>\n    <p>tail</p>\n  </dd>\n</dl>"
    );
}

#[test]
fn a_closed_attached_fence_keeps_its_interior_blank() {
    let html = carve::to_html("[^1]: +\n```\n\n  tail\n```\n\n[^1]\n");
    assert!(
        html.contains("<pre><code>\n  tail\n</code></pre>"),
        "{html}"
    );
}
