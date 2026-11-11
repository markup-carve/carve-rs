//! Remaining ownership-table cells from carve#5658.

#[test]
fn note_marker_unclosed_fence_refused_plus_0() {
    let source = "see[^a]\n\n[^a]: +\n```\na\n\n+\n  zq";
    let expected = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <pre><code>a\n</code></pre>\n      <p>zq<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn note_stacked_unclosed_fence_refused_plus_1() {
    let source = "see[^a]\n\n- [^a]: +\n  ```\n  a\n\n  +\n    zq";
    let expected = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<ul>\n  <li></li>\n</ul>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <pre><code>a\n</code></pre>\n      <p>zq<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn dd_marker_unclosed_fence_refused_plus_2() {
    let source = ":: t\n: +\n```\na\n\n+\n  zq";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code>a\n</code></pre>\n    <p>zq</p>\n  </dd>\n</dl>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn dd_stacked_unclosed_fence_refused_plus_3() {
    let source = "- :: t\n  : +\n  ```\n  a\n\n  +\n    zq";
    let expected = "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n      <dd>\n        <pre><code>a\n</code></pre>\n        <p>zq</p>\n      </dd>\n    </dl>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim(), expected);
}
#[test]
fn quote_own_fence_0() {
    let source = "> p\n+\n```\na\n\n>   zq";
    let expected = "<blockquote>\n  <p>p</p>\n  <pre><code>a\n\n  zq</code></pre>\n</blockquote>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn quote_own_fence_1() {
    let source = "> p\n+\n```\na\n\n> zq";
    let expected = "<blockquote>\n  <p>p</p>\n  <pre><code>a\n\nzq</code></pre>\n</blockquote>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn quote_own_fence_2() {
    let source = "> p\n+\n```\na\n\n> +\n>   zq";
    let expected =
        "<blockquote>\n  <p>p</p>\n  <pre><code>a\n\n+\n  zq</code></pre>\n</blockquote>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn quote_own_fence_3() {
    let source = "> p\n+\n```\na\n\n>  ```\n>   zq";
    let expected =
        "<blockquote>\n  <p>p</p>\n  <pre><code>a\n\n ```\n  zq</code></pre>\n</blockquote>";
    assert_eq!(carve::to_html(source).trim(), expected);
}
#[test]
fn own_fence_shifted_0() {
    let source = "- p\n+\n```\na\n\n ```\n  zq";
    let expected = "<ul>\n  <li>p\n    <pre><code>a\n\n</code></pre>\n  </li>\n</ul>\n<pre><code> zq</code></pre>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn own_fence_shifted_1() {
    let source = "* p\n+\n```\na\n\n ```\n  zq";
    let expected = "<ul>\n  <li>p\n    <pre><code>a\n\n</code></pre>\n  </li>\n</ul>\n<pre><code> zq</code></pre>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn own_fence_shifted_2() {
    let source = "see[^a]\n\n[^a]: p\n+\n```\na\n\n ```\n  zq";
    let expected = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<pre><code> zq</code></pre>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>p</p>\n      <pre><code>a\n\n</code></pre>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn own_fence_shifted_3() {
    let source = ":: t\n: p\n+\n```\na\n\n ```\n  zq";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>p</p>\n    <pre><code>a\n\n</code></pre>\n  </dd>\n</dl>\n<pre><code> zq</code></pre>";
    assert_eq!(carve::to_html(source).trim(), expected);
}
#[test]
fn stacked_code_attribute_0() {
    let source = "- - +\n  `o\n  {#i\n   .y}\nzq";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li><code>o\n{#i\n.y}\nzq</code></li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn stacked_code_attribute_1() {
    let source = "* - +\n  `o\n  {#i\n   .y}\nzq";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li><code>o\n{#i\n.y}\nzq</code></li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn stacked_code_attribute_2() {
    let source = "- :: t\n  : +\n  `o\n  {#i\n   .y}\nzq";
    let expected = "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n      <dd><code>o\n{#i\n.y}\nzq</code></dd>\n    </dl>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source).trim(), expected);
}
#[test]
fn stacked_quote_attribute_code_span_releases_its_marker_column() {
    let source = "- > +\n  `o\n  {#i\n   .y}\nzq";
    let expected = "<ul>\n  <li>\n    <blockquote><p><code>o\n{#i\n.y}</code></p></blockquote>\n  </li>\n</ul>\n<p>zq</p>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn final_fence_0() {
    let source = "see[^a]\n\n[^a]: p\n+\n```\na\n\n+\n  zq";
    let expected = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>p</p>\n      <pre><code>a\n\n</code></pre>\n      <p>zq<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn final_fence_1() {
    let source = "see[^a]\n\n- [^a]: +\n  ```\n  a\nzq";
    let expected = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<ul>\n  <li></li>\n</ul>\n<p>zq</p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <pre><code>a\n</code></pre>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn final_fence_2() {
    let source = "see[^a]\n\n- [^a]: +\n  ```\n  a\n\n   ```\n    zq";
    let expected = "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<ul>\n  <li></li>\n</ul>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <pre><code>a\n\n</code></pre>\n      <p>zq<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn final_fence_3() {
    let source = ":: t\n: p\n+\n```\na\n\n+\n  zq";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>p</p>\n    <pre><code>a\n\n</code></pre>\n    <p>zq</p>\n  </dd>\n</dl>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn quote_fence_tail_ends_before_document_sibling() {
    let source = "> p\n+\n```\na\n\n>  ```\n>   zq\n\noutside0";
    let expected = "<blockquote>\n  <p>p</p>\n  <pre><code>a\n\n ```\n  zq\n</code></pre>\n</blockquote>\n<p>outside0</p>";
    assert_eq!(carve::to_html(source).trim(), expected);
}
