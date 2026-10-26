#[test]
fn an_unclosed_code_run_keeps_its_nested_item_paragraph() {
    for (source, expected) in [
        (
            ":: term\n: - a\n    ```\nx\n+ c",
            "<dl>\n  <dt>term</dt>\n  <dd>\n    <ul>\n      <li>a\n<code>\nx\n+ c</code></li>\n    </ul>\n  </dd>\n</dl>",
        ),
        (
            ":: term\n: - a\n    ````\nx\n+ c",
            "<dl>\n  <dt>term</dt>\n  <dd>\n    <ul>\n      <li>a\n<code>\nx\n+ c</code></li>\n    </ul>\n  </dd>\n</dl>",
        ),
        (
            ":: term\n: 1. a\n     ```\nx\n+ c",
            "<dl>\n  <dt>term</dt>\n  <dd>\n    <ol>\n      <li>a\n<code>\nx\n+ c</code></li>\n    </ol>\n  </dd>\n</dl>",
        ),
    ] {
        assert_eq!(carve::to_html(source).trim(), expected, "{source:?}");
    }
}
