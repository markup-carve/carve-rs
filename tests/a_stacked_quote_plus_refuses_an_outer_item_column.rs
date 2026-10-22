//! A stacked first-block `+` in a quote takes a follower only at column 0 or its
//! line's column; at an outer item's column it is refused and reads like a
//! letter (markup-carve/carve#3421).

use carve::to_html;

fn flat(source: &str) -> String {
    to_html(source).split_whitespace().collect()
}

#[test]
fn a_follower_at_an_outer_item_column_folds_into_the_plus() {
    for (source, expected) in [
        (
            "- - > +\n  b\n",
            "<ul><li><ul><li><blockquote><p>+b</p></blockquote></li></ul></li></ul>",
        ),
        (
            "- > > +\n  b\n",
            "<ul><li><blockquote><blockquote><p>+b</p></blockquote></blockquote></li></ul>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn column_zero_and_the_unquoted_twin_keep_their_reading() {
    for (source, expected) in [
        (
            "- - > +\nb\n",
            "<ul><li><ul><li><blockquote><p>b</p></blockquote></li></ul></li></ul>",
        ),
        ("- - +\n  b\n", "<ul><li><ul><li></li></ul>b</li></ul>"),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}
