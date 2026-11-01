use carve::to_html;

#[test]
fn captions_in_definition_bodies_take_lazy_lines() {
    for (source, expected) in [
        (
            "[^0]: ```\n  ```\n  ^ p\nz\n\n[^0]\n",
            "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <figure>\n        <pre><code></code></pre>\n        <figcaption>p\nz</figcaption>\n      </figure>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
        ),
        (
            ":: t\n: ```\n  ```\n  ^ p\nz\n",
            "<dl>\n  <dt>t</dt>\n  <dd>\n    <figure>\n      <pre><code></code></pre>\n      <figcaption>p\nz</figcaption>\n    </figure>\n  </dd>\n</dl>",
        ),
        (
            "- :: t\n  : ```\n    ```\n    ^ p\nz\n",
            "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n      <dd>\n        <figure>\n          <pre><code></code></pre>\n          <figcaption>p\nz</figcaption>\n        </figure>\n      </dd>\n    </dl>\n  </li>\n</ul>",
        ),
    ] {
        assert_eq!(to_html(source), expected, "{source:?}");
    }
}
