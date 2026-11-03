//! A `%%%` span opened on its own line inside a quoted item or description
//! body ends at a line that misses that body (PART 0 S1), with or without a
//! closer (carve#4907), so an unmarked line after it continues the quote's
//! paragraph (carve#5010). Expected HTML is the spec model's at carve `6c163012`.

fn flat(source: &str) -> String {
    carve::to_html(source)
        .split('\n')
        .map(str::trim)
        .collect::<String>()
}

#[test]
fn a_line_after_a_span_in_an_empty_stanza_continues_the_quote() {
    let cases = [
        (
            "> - ::: |\n>   %%%\n>   :::\n>   :::bad\n> c\n- tail\n",
            "<blockquote><ul><li><div class=\"line-block\"></div></li></ul><p>c- tail</p></blockquote>",
        ),
        (
            "> - ::: |\n>   %%%\n> c\n- tail\n",
            "<blockquote><ul><li><div class=\"line-block\"></div></li></ul><p>c- tail</p></blockquote>",
        ),
        (
            "> - ::: |\n>   %%%\n> c\nzz\n",
            "<blockquote><ul><li><div class=\"line-block\"></div></li></ul><p>czz</p></blockquote>",
        ),
    ];
    for (source, html) in cases {
        assert_eq!(flat(source), html, "{source:?}");
    }
}

#[test]
fn a_span_in_a_quoted_body_ends_at_a_line_left_of_the_body() {
    let cases = [
        (
            "> - a\n>   %%%\n> c\nzz\n",
            "<blockquote><ul><li>a</li></ul><p>czz</p></blockquote>",
        ),
        (
            "> - a\n>\n>   %%%\n> c\nzz\n",
            "<blockquote><ul><li>a</li></ul><p>czz</p></blockquote>",
        ),
        (
            "> :: t\n> : d\n>   %%%\n> c\nzz\n",
            "<blockquote><dl><dt>t</dt><dd>d</dd></dl><p>czz</p></blockquote>",
        ),
    ];
    for (source, html) in cases {
        assert_eq!(flat(source), html, "{source:?}");
    }
}

#[test]
fn a_stanza_with_verse_still_takes_the_line_and_ends_at_the_marker() {
    assert_eq!(
        flat("> - ::: |\n>   x\n>   %%%\n> c\n- tail\n"),
        "<blockquote><ul><li><div class=\"line-block\"><p>x<br>c</p></div></li></ul></blockquote><ul><li>tail</li></ul>",
    );
}
