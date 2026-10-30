//! An unclosed fence-shaped line below a quote that an item's marker line opens
//! is lazy text of the quote's paragraph, as the spec model and carve-js read it
//! (markup-carve/carve#3421).

use carve::to_html;

fn flat(source: &str) -> String {
    to_html(source).split_whitespace().collect()
}

#[test]
fn the_fence_line_folds_into_the_quoted_paragraph() {
    for (source, expected) in [
        (
            "- - > x\n   ```\nx\n```\n",
            "<ul><li><ul><li><blockquote><p>x</p></blockquote></li></ul><pre><code></code></pre></li></ul><p>x</p><pre><code></code></pre>",
        ),
        (
            "- - > +\n   ```\nx\n```\n",
            "<ul><li><ul><li><blockquote><p>+</p></blockquote></li></ul><pre><code></code></pre></li></ul><p>x</p><pre><code></code></pre>",
        ),
        (
            "- 1. > +\n    ```\nx\n```\n",
            "<ul><li><ol><li><blockquote><p>+</p></blockquote></li></ol><pre><code></code></pre></li></ul><p>x</p><pre><code></code></pre>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn a_fence_closed_inside_the_item_still_opens() {
    assert_eq!(
        flat("- - > x\n   ```\n   y\n   ```\n"),
        "<ul><li><ul><li><blockquote><p>x</p></blockquote></li></ul><pre><code>y</code></pre></li></ul>",
    );
}

#[test]
fn a_fence_below_an_indented_quote_plus_and_a_nested_item_quote() {
    for (source, expected) in [
        (
            "- a\n  > +\n  ```\nx\n```\n",
            "<ul><li>a<blockquote><pre><code></code></pre></blockquote></li></ul><p>x</p><pre><code></code></pre>",
        ),
        (
            "- a\n  > +\n  ```\n  x\n  ```\n",
            "<ul><li>a<blockquote><pre><code>x</code></pre></blockquote></li></ul>",
        ),
        (
            "- a\n  - > +\n   ```\nx\n```\n",
            "<ul><li>a<ul><li><blockquote><p>+</p></blockquote></li></ul><pre><code></code></pre></li></ul><p>x</p><pre><code></code></pre>",
        ),
        (
            "- - > +\n    ```\nx\n```\n",
            "<ul><li><ul><li><blockquote><pre><code></code></pre></blockquote></li></ul></li></ul><p>x</p><pre><code></code></pre>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}
