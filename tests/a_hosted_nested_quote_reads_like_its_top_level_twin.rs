use carve::{to_html, to_html_with_options, Options};

fn compact(html: &str) -> String {
    html.lines().map(str::trim).collect::<Vec<_>>().join("")
}

#[test]
fn hosted_nested_quote_reads_like_its_top_level_twin() {
    let cases = [
        ("> > q\n> +\n> para\n", "<blockquote><blockquote><p>q</p><p>para</p></blockquote></blockquote>"),
        ("- > > q\n  > +\n  > para\n", "<ul><li><blockquote><blockquote><p>q</p><p>para</p></blockquote></blockquote></li></ul>"),
        ("1. > > q\n   > +\n   > para\n", "<ol><li><blockquote><blockquote><p>q</p><p>para</p></blockquote></blockquote></li></ol>"),
        ("- > > > q\n  > > +\n  > > para\n", "<ul><li><blockquote><blockquote><blockquote><p>q</p><p>para</p></blockquote></blockquote></blockquote></li></ul>"),
        ("- - > > q\n    > +\n    > para\n", "<ul><li><ul><li><blockquote><blockquote><p>q</p><p>para</p></blockquote></blockquote></li></ul></li></ul>"),
        ("- x\n  > > q\n  > +\n  > para\n", "<ul><li>x<blockquote><blockquote><p>q</p><p>para</p></blockquote></blockquote></li></ul>"),
        ("> - > > q\n>   > +\n>   > para\n", "<blockquote><ul><li><blockquote><blockquote><p>q</p><p>para</p></blockquote></blockquote></li></ul></blockquote>"),
        (":: t\n:  > > q\n   > +\n   > para\n", "<dl><dt>t</dt><dd><blockquote><blockquote><p>q</p><p>para</p></blockquote></blockquote></dd></dl>"),
    ];
    for (source, expected) in cases {
        assert_eq!(compact(&to_html(source)), expected, "{source:?}");
        assert_eq!(
            compact(&to_html_with_options(
                source,
                &Options::new().with_positions(true)
            )),
            expected,
            "with positions: {source:?}"
        );
    }
}

#[test]
fn a_bare_plus_before_a_sibling_item_stays_text_in_a_described_quote() {
    assert_eq!(
        compact(&to_html(":: t\n:  > > > q\n   +\n   - b\n")),
        "<dl><dt>t</dt><dd><blockquote><blockquote><blockquote><p>q</p></blockquote></blockquote><ul><li>b</li></ul></blockquote></dd></dl>"
    );
}
