//! Pack day-one drift fixed under carve-rs#3534: each shape renders as the spec model.

fn html(source: &str) -> String {
    carve::to_html(source).trim_end().to_string()
}

/// A run after a blank that starts indented still belongs to the item above, so a
/// sibling marker line opens an item and its definition is collected.
#[test]
fn a_marker_line_definition_after_an_indented_run_is_a_definition() {
    for (source, expected) in [
        (
            "- b\n\n  p\n* [d]: /u\n\n[x][d]\n",
            "<ul>\n  <li><p>b</p>\n    <p>p</p>\n  </li>\n</ul>\n<ul>\n  <li></li>\n</ul>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "1. b\n\n   p\n2. [d]:\n",
            "<ol>\n  <li><p>b</p>\n    <p>p</p>\n  </li>\n  <li></li>\n</ol>",
        ),
        (
            "- b\n\n  > p\n- [^d]: x\n",
            "<ul>\n  <li>b\n    <blockquote><p>p</p></blockquote>\n  </li>\n  <li></li>\n</ul>",
        ),
        (
            "::: d\n- a\n\n  b\n- [d]: /u\n:::\n",
            "<div class=\"d\">\n  <ul>\n    <li><p>a</p>\n      <p>b</p>\n    </li>\n    <li></li>\n  </ul>\n</div>",
        ),
    ] {
        assert_eq!(html(source), expected, "{source:?}");
    }
}

/// An unclosed code fence in a div in an item holds no paragraph, so a line below
/// the item column ends the item.
#[test]
fn an_open_code_fence_in_a_div_takes_no_lazy_line() {
    for (source, expected) in [
        (
            "- a\n  :::\n  ```\n  p\nt\n",
            "<ul>\n  <li>a\n    <div>\n      <pre><code>p\n</code></pre>\n    </div>\n  </li>\n</ul>\n<p>t</p>",
        ),
        (
            "- a\n  :::\n  q\n  ~~~\n  p\nt\n",
            "<ul>\n  <li>a\n    <div>\n      <p>q</p>\n      <pre><code>p\n</code></pre>\n    </div>\n  </li>\n</ul>\n<p>t</p>",
        ),
        (
            "- a\n  - b\n    :::\n    ```\n    p\n  t\n",
            "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <div>\n          <pre><code>p\n</code></pre>\n        </div>\n      </li>\n    </ul>\n    t\n  </li>\n</ul>",
        ),
        (
            "- - a\n    :::\n    ```\n    p\n t\n",
            "<ul>\n  <li>\n    <ul>\n      <li>a\n        <div>\n          <pre><code>p\n</code></pre>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>t</p>",
        ),
        (
            "- b\n  - a\n    :::\n    q\n    ```\n    p\n t\n",
            "<ul>\n  <li>b\n    <ul>\n      <li>a\n        <div>\n          <p>q</p>\n          <pre><code>p\n</code></pre>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>t</p>",
        ),
    ] {
        assert_eq!(html(source), expected, "{source:?}");
    }
}

/// A `+` the layout gives to the container holding a note continues that container,
/// not the note body.
#[test]
fn a_plus_owned_by_an_outer_host_leaves_the_nested_note() {
    for (source, expected) in [
        (
            "> [^f]: n\n+\nt\n\n[^f]\n",
            "<blockquote><p>t</p></blockquote>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>n<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
        ),
        (
            "- [^f]: n\n+\nt\n\n[^f]\n",
            "<ul>\n  <li>t</li>\n</ul>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>n<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
        ),
        (
            ":: x\n: [^f]: n\n+\nt\n\n[^f]\n",
            "<dl>\n  <dt>x</dt>\n  <dd>t</dd>\n</dl>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>n<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
        ),
        (
            "> [^f]: n\n  m\n+\nt\n\n[^f]\n",
            "<blockquote><p>t</p></blockquote>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>n\nm<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
        ),
    ] {
        assert_eq!(html(source), expected, "{source:?}");
    }
}
