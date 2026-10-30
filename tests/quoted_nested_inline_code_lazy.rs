#[test]
fn paired_inline_code_runs_keep_nested_item_lazy_claim() {
    assert_eq!(
        carve::to_html("> - a\n>     - child\n>     ```\n>       ```\nafter"),
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>child</li>\n      </ul>\n      <pre><code>  ```\n</code></pre>\n    </li>\n  </ul>\n</blockquote>\n<p>after</p>"
    );
}
