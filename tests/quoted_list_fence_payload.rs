#[test]
fn overindented_payload_stays_in_marker_line_quoted_fence() {
    assert_eq!(
        carve::to_html("- > ```\n    > y\n  > ```"),
        "<ul>\n  <li>\n    <blockquote>\n      <pre><code>y\n</code></pre>\n    </blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn second_quoted_fence_keeps_overindented_payload() {
    assert_eq!(
        carve::to_html("- > ```\n  > x\n  > ```\n  > ```\n    > y"),
        "<ul>\n  <li>\n    <blockquote>\n      <pre><code>x\n</code></pre>\n      <pre><code>y</code></pre>\n    </blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn tilde_quoted_fence_keeps_overindented_payload() {
    assert_eq!(
        carve::to_html("- > ~~~\n    > y\n  > ~~~"),
        "<ul>\n  <li>\n    <blockquote>\n      <pre><code>y\n</code></pre>\n    </blockquote>\n  </li>\n</ul>"
    );
}
