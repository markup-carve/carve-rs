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

#[test]
fn a_stacked_quote_takes_a_list_at_its_owning_quote_column() {
    let expected =
        "<ul><li><blockquote><blockquote><ul><li>s</li></ul></blockquote></blockquote></li></ul>";
    assert_eq!(flat("- > +\n  > - s\n"), expected);
    assert_eq!(
        flat("> - > +\n>   > - s\n"),
        format!("<blockquote>{expected}</blockquote>")
    );
}

#[test]
fn an_attached_colon_fence_closes_at_its_indented_closer() {
    for (source, expected) in [
        (
            "- > +\n  ::: n\n  b\n  :::\n\nz\n",
            "<ul><li><blockquote><divclass=\"n\"><p>b</p></div></blockquote></li></ul><p>z</p>",
        ),
        (
            "- a\n  - > +\n    ::: n\n    b\n    :::\n\nz\n",
            "<ul><li>a<ul><li><blockquote><divclass=\"n\"><p>b</p></div></blockquote></li></ul></li></ul><p>z</p>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn a_stacked_opener_takes_any_block_at_its_owning_quote_column() {
    for (source, expected) in [
        (
            "- - > +\n    # h\n",
            "<ul><li><ul><li><blockquote><h1id=\"h\">h</h1></blockquote></li></ul></li></ul>",
        ),
        (
            "- > > +\n  > q\n",
            "<ul><li><blockquote><blockquote><p>+q</p></blockquote></blockquote></li></ul>",
        ),
        (
            "- [ ] > +\n      # h\n",
            "<ulclass=\"task-list\"><li><inputtype=\"checkbox\"disabled><blockquote><p>+</p></blockquote><h1id=\"h\">h</h1></li></ul>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}
