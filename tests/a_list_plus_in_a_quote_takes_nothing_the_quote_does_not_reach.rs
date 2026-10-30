//! A first-block list `+` inside a quote takes no follower the quote does not
//! reach: the item stays empty and the follower is read after the quote, as
//! the spec model and carve-js read it (markup-carve/carve#3421).

use carve::to_html;

fn flat(source: &str) -> String {
    to_html(source).split_whitespace().collect()
}

#[test]
fn an_unprefixed_follower_ends_the_quote() {
    for (source, expected) in [
        ("> - +\nb\n", "<blockquote><ul><li>+b</li></ul></blockquote>"),
        (
            "> > - +\nb\n",
            "<blockquote><blockquote><ul><li>+b</li></ul></blockquote></blockquote>",
        ),
        (
            "- a\n  > - +\nb\n",
            "<ul><li>a<blockquote><ul><li>+b</li></ul></blockquote></li></ul>",
        ),
        (
            "> 1. +\n | c |\n",
            "<blockquote><ol><li>+</li></ol></blockquote><table><tbody><tr><td>c</td></tr></tbody></table>",
        ),
        (
            "- a\n  > - +\n> q\n",
            "<ul><li>a<blockquote><ul><li>+</li></ul></blockquote></li></ul><blockquote><p>q</p></blockquote>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn a_follower_past_the_quote_joins_the_enclosing_item() {
    for (source, expected) in [
        (
            "- > - +\nb\n",
            "<ul><li><blockquote><ul><li>+b</li></ul></blockquote></li></ul>",
        ),
        (
            "> - > - +\n> b\n",
            "<blockquote><ul><li><blockquote><ul><li>+b</li></ul></blockquote></li></ul></blockquote>",
        ),
        (
            "- > - +\n    - s\n",
            "<ul><li><blockquote><ul><li>+-s</li></ul></blockquote></li></ul>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn a_follower_inside_the_quote_keeps_its_reading() {
    for (source, expected) in [
        (
            "> - +\n> b\n",
            "<blockquote><ul><li>b</li></ul></blockquote>",
        ),
        (
            "> - +\n>  b\n",
            "<blockquote><ul><li>+b</li></ul></blockquote>",
        ),
        (
            "> - +\n>   b\n",
            "<blockquote><ul><li>+b</li></ul></blockquote>",
        ),
        ("- - +\nb\n", "<ul><li><ul><li>+b</li></ul></li></ul>"),
        ("> a\n> - +\nb\n", "<blockquote><p>a-+b</p></blockquote>"),
        (
            "> > a\n> > - +\nb\n",
            "<blockquote><blockquote><p>a-+b</p></blockquote></blockquote>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn an_indented_quote_marker_below_the_quote_is_item_text() {
    for (source, expected) in [
        (
            "- a\n  > - - +\n   > q\n",
            "<ul><li>a<blockquote><ul><li><ul><li>+q</li></ul></li></ul></blockquote></li></ul>",
        ),
        (
            "- a\n  > - - +\n      > q\n",
            "<ul><li>a<blockquote><ul><li><ul><li>+q</li></ul></li></ul></blockquote></li></ul>",
        ),
        (
            "- a\n   > - +\n   > - b\n",
            "<ul><li>a<blockquote><ul><li>+</li><li>b</li></ul></blockquote></li></ul>",
        ),
        (
            "- a\n  > - - +\n  > q\n",
            "<ul><li>a<blockquote><ul><li><ul><li>+q</li></ul></li></ul></blockquote></li></ul>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn a_nested_item_and_its_first_block_readings() {
    for (source, expected) in [
        (
            "- a\n  - > - +\nb\n",
            "<ul><li>a<ul><li><blockquote><ul><li>+b</li></ul></blockquote></li></ul></li></ul>",
        ),
        (
            "- a\n  - > - +\n b\n",
            "<ul><li>a<ul><li><blockquote><ul><li>+b</li></ul></blockquote></li></ul></li></ul>",
        ),
        (
            "- a\n  - > - +\n - s\n",
            "<ul><li>a<ul><li><blockquote><ul><li>+</li></ul></blockquote></li></ul></li><li>s</li></ul>",
        ),
        (
            "- a\n  - - +\n```\nx\n```\n",
            "<ul><li>a<ul><li><ul><li>+</li></ul></li></ul></li></ul><pre><code>x</code></pre>",
        ),
        (
            "- a\n  > > > +\n  > q\n",
            "<ul><li>a<blockquote><blockquote><blockquote><p>+q</p></blockquote></blockquote></blockquote></li></ul>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}
