//! A quote opened by `> +` on an item's marker line takes a non-list block at
//! its own column, and a refused `+` there stays text that a lazy line folds
//! into, like a letter (CARVE-P9-067, markup-carve/carve#3421).

use carve::to_html;

fn flat(source: &str) -> String {
    to_html(source).split_whitespace().collect()
}

#[test]
fn a_block_at_the_quote_column_attaches() {
    for (source, expected) in [
        ("- > +\n  | c |\n", "<ul><li><blockquote><table><tbody><tr><td>c</td></tr></tbody></table></blockquote></li></ul>"),
        ("- a\n  - > +\n    | c |\n", "<ul><li>a<ul><li><blockquote><table><tbody><tr><td>c</td></tr></tbody></table></blockquote></li></ul></li></ul>"),
        ("- > +\n  # h\n", "<ul><li><blockquote><h1id=\"h\">h</h1></blockquote></li></ul>"),
        ("- > +\n  ```\n  x\n  ```\n", "<ul><li><blockquote><pre><code>x</code></pre></blockquote></li></ul>"),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn a_refused_plus_folds_a_lazy_line_like_a_letter() {
    for (source, expected) in [
        (
            "- > +\n b\n",
            "<ul><li><blockquote><p>+b</p></blockquote></li></ul>",
        ),
        (
            "- a\n  - > +\n b\n",
            "<ul><li>a<ul><li><blockquote><p>+b</p></blockquote></li></ul></li></ul>",
        ),
        (
            "- a\n  - > +\n   b\n",
            "<ul><li>a<ul><li><blockquote><p>+b</p></blockquote></li></ul></li></ul>",
        ),
        (
            "- a\n  - > +\n      b\n",
            "<ul><li>a<ul><li><blockquote><p>+b</p></blockquote></li></ul></li></ul>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn a_list_item_at_the_quote_column_is_not_pulled() {
    assert_eq!(
        flat("- a\n  - > +\n    - b\n"),
        "<ul><li>a<ul><li><blockquote><p>+-b</p></blockquote></li></ul></li></ul>"
    );
}
