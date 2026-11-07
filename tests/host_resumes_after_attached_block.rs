#[test]
fn item_resumes_after_a_closed_attached_fence() {
    let source = "- +\n```\n```\n\n  - x\n";
    let expected = "<ul>\n  <li>\n    <pre><code></code></pre>\n    <ul>\n      <li>x</li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn note_resumes_after_an_attached_heading_and_blank() {
    let source = "[^1]: +\n# h\n\n  - y\n\n[^1]\n";
    let html = carve::to_html(source);
    assert!(html.contains("<li id=\"fn1\">\n      <h1 id=\"h\">h</h1>\n      <ul>\n        <li>y</li>\n      </ul>"), "{html}");
}

#[test]
fn description_resumes_after_an_attached_colon_block_and_blank() {
    let source = ":: t\n: +\n::: x\n:::\n\n  > z\n";
    let html = carve::to_html(source);
    assert!(html.contains("<dd>\n    <div class=\"x\">\n\n    </div>\n    <blockquote><p>z</p></blockquote>\n  </dd>"), "{html}");
}
