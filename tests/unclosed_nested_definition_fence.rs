#[test]
fn an_unclosed_code_run_keeps_its_nested_item_paragraph() {
    for (source, expected) in [
        (
            ":: term\n: - a\n    ```\nx\n+ c",
            "<dl>\n  <dt>term</dt>\n  <dd>\n    <ul>\n      <li>a\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>\n<p>x\n+ c</p>",
        ),
        (
            ":: term\n: - a\n    ````\nx\n+ c",
            "<dl>\n  <dt>term</dt>\n  <dd>\n    <ul>\n      <li>a\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>\n<p>x\n+ c</p>",
        ),
        (
            ":: term\n: 1. a\n     ```\nx\n+ c",
            "<dl>\n  <dt>term</dt>\n  <dd>\n    <ol>\n      <li>a\n        <pre><code></code></pre>\n      </li>\n    </ol>\n  </dd>\n</dl>\n<p>x\n+ c</p>",
        ),
    ] {
        assert_eq!(carve::to_html(source).trim(), expected, "{source:?}");
    }
}
