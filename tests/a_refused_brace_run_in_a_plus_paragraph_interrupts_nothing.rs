//! The line after a refused brace run continues the run's paragraph, also in a
//! column-0 paragraph a `+` attaches to an item, and a fence there keeps its
//! closer (CARVE-P2-035, markup-carve/carve-rs#3875).

fn html(src: &str) -> String {
    carve::to_html(src)
}

#[test]
fn a_marker_at_the_content_column_stays_text() {
    assert_eq!(
        html("- a\n+\n{x\ny}\n  - c\n"),
        "<ul>\n  <li>a\n    {x\ny}\n    <ul>\n      <li>c</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_quote_at_the_content_column_stays_text() {
    assert_eq!(
        html("- a\n+\n{x\ny}\n  > c\n"),
        "<ul>\n  <li>a\n    {x\ny}\n    <blockquote><p>c</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn a_fence_keeps_its_closer_in_the_paragraph() {
    assert_eq!(
        html("- a\n+\n{x\ny}\n  ```\n  k\n  ```\n"),
        "<ul>\n  <li>a\n    {x\ny}\n    <pre><code>k\n</code></pre>\n  </li>\n</ul>"
    );
}

#[test]
fn only_the_next_line_is_forced() {
    assert_eq!(
        html("- a\n+\n{x\ny}\n  - c\n  - d\n"),
        "<ul>\n  <li>a\n    {x\ny}\n    <ul>\n      <li>c</li>\n      <li>d</li>\n    </ul>\n  </li>\n</ul>"
    );
}
