#[test]
fn paired_inline_code_runs_keep_nested_item_lazy_claim() {
    assert_eq!(
        carve::to_html("> - a\n>     - child\n>     ```\n>       ```\nafter"),
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>child\n<code>\n</code>\nafter</li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>"
    );
}
