// carve#4449 (ruled B): a quote's `+` attaches the first visible line past
// comment lines, closed comment spans and definitions; they stay in the quote.

fn flat(source: &str) -> String {
    carve::to_html(source)
        .split('\n')
        .map(str::trim)
        .collect::<String>()
}

#[test]
fn the_block_past_the_invisible_lines_joins_the_quote() {
    for invisible in ["%% c", "%%%\nc\n%%%", "%% c\n%% d", "[r]: /u"] {
        let source = format!("> +\n{invisible}\ntext\n");
        assert_eq!(
            flat(&source),
            "<blockquote><p>text</p></blockquote>",
            "{source:?}"
        );
    }
}

#[test]
fn a_nested_quote_still_refuses() {
    assert_eq!(
        flat("> > +\n%% c\ntext\n"),
        "<blockquote><blockquote></blockquote></blockquote><p>text</p>"
    );
}
