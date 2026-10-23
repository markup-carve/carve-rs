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
            "<ul><li><ul><li><blockquote><p>x<code>x</code></p></blockquote></li></ul></li></ul>",
        ),
        (
            "- - > +\n   ```\nx\n```\n",
            "<ul><li><ul><li><blockquote><p>+<code>x</code></p></blockquote></li></ul></li></ul>",
        ),
        (
            "- 1. > +\n    ```\nx\n```\n",
            "<ul><li><ol><li><blockquote><p>+<code>x</code></p></blockquote></li></ol></li></ul>",
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
