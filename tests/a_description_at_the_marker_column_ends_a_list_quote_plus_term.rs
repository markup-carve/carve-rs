//! A `: ` line at the marker column ends an item whose `> +` attached an
//! unquoted definition list (PART 0 S4, markup-carve/carve#5317,
//! markup-carve/carve-rs#4009).

use carve::to_html;

fn flat(source: &str) -> String {
    to_html(source).split_whitespace().collect()
}

#[test]
fn the_flush_description_line_ends_the_item() {
    for (source, expected) in [
        (
            "- > +\n  :: t\n: d\n",
            "<ul><li><blockquote><dl><dt>t</dt></dl></blockquote></li></ul><p>:d</p>",
        ),
        (
            "- > +\n  :: t\n  : d\n: d\n",
            "<ul><li><blockquote><dl><dt>t</dt><dd>d</dd></dl></blockquote></li></ul><p>:d</p>",
        ),
        (
            "1. > +\n   :: t\n   : d\n: d\n",
            "<ol><li><blockquote><dl><dt>t</dt><dd>d</dd></dl></blockquote></li></ol><p>:d</p>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn a_quoted_term_takes_the_line_lazily() {
    let lazy = "<ul><li><blockquote><blockquote><dl><dt>t</dt><dd>d:d</dd></dl></blockquote></blockquote></li></ul>";
    for source in [
        "- > +\n  > :: t\n  > : d\n: d\n",
        "- > +\n  %%%\n  %%%\n  > :: t\n  > : d\n: d\n",
    ] {
        assert_eq!(flat(source), lazy, "{source:?}");
    }
}

#[test]
fn an_open_description_still_takes_plain_lazy_text() {
    assert_eq!(
        flat("- > +\n  :: t\n  : d\nz\n"),
        "<ul><li><blockquote><dl><dt>t</dt><dd>dz</dd></dl></blockquote></li></ul>"
    );
}
