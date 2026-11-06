//! A `+` in a quote on a list marker line attaches one block, measured at that
//! block's own column. Only an open paragraph takes the next unmarked line
//! (PART 0 S1, S4, markup-carve/carve#5330).

use carve::to_html;

fn flat(source: &str) -> String {
    to_html(source).split_whitespace().collect()
}

#[test]
fn a_closed_attached_block_leaves_the_flush_line_to_the_document() {
    let nested_empty =
        "<ul><li><blockquote><blockquote></blockquote></blockquote></li></ul><p>after</p>";
    for (source, expected) in [
        ("- > +\n  > %%%\n  > k\nafter\n", nested_empty),
        ("- > +\n  > %%%\nafter\n", nested_empty),
        ("- > +\n  > %%%\n  > k\n  > %%%\nafter\n", nested_empty),
        ("1. > +\n   > %%%\n   > k\nafter\n", "<ol><li><blockquote><blockquote></blockquote></blockquote></li></ol><p>after</p>"),
        (
            "- > +\n  > # h\nafter\n",
            "<ul><li><blockquote><blockquote><h1id=\"h\">h</h1></blockquote></blockquote></li></ul><p>after</p>",
        ),
        (
            "- > +\n  > ```\n  > k\nafter\n",
            "<ul><li><blockquote><blockquote><pre><code>k</code></pre></blockquote></blockquote></li></ul><p>after</p>",
        ),
        (
            "- > +\n  # h\nafter\n",
            "<ul><li><blockquote><h1id=\"h\">h</h1></blockquote></li></ul><p>after</p>",
        ),
        (
            "- [ ] > +\n      > # h\nafter\n",
            "<ulclass=\"task-list\"><li><inputtype=\"checkbox\"disabled><blockquote><blockquote><h1id=\"h\">h</h1></blockquote></blockquote></li></ul><p>after</p>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn a_nested_marker_quote_plus_ends_both_items() {
    for (source, expected) in [
        (
            "- - > +\n    > %%%\n    > k\nafter\n",
            "<ul><li><ul><li><blockquote><blockquote></blockquote></blockquote></li></ul></li></ul><p>after</p>",
        ),
        (
            "- - > +\n    > # h\nafter\n",
            "<ul><li><ul><li><blockquote><blockquote><h1id=\"h\">h</h1></blockquote></blockquote></li></ul></li></ul><p>after</p>",
        ),
        (
            "- - > +\n    # h\nafter\n",
            "<ul><li><ul><li><blockquote><h1id=\"h\">h</h1></blockquote></li></ul></li></ul><p>after</p>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn a_line_at_the_item_column_stays_item_text() {
    assert_eq!(
        flat("- > +\n  > %%%\n  > k\n  after\n"),
        "<ul><li><blockquote><blockquote></blockquote></blockquote>after</li></ul>"
    );
}

#[test]
fn an_attached_paragraph_still_takes_its_lazy_line() {
    for (source, expected) in [
        (
            "- > +\n  > text\nafter\n",
            "<ul><li><blockquote><blockquote><p>textafter</p></blockquote></blockquote></li></ul>",
        ),
        (
            "- - > +\n    > text\nafter\n",
            "<ul><li><ul><li><blockquote><blockquote><p>textafter</p></blockquote></blockquote></li></ul></li></ul>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}
