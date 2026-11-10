use carve::to_html;

#[test]
fn unprefixed_continuation_closes_each_quote_host() {
    let cases = [
        (
            "> {.x\n   .y}\nplain\n",
            "<blockquote>\n\n</blockquote>\n<p>plain</p>",
        ),
        (
            "> > {.x\n     .y}\nplain\n",
            "<blockquote>\n  <blockquote>\n\n  </blockquote>\n</blockquote>\n<p>plain</p>",
        ),
        (
            "> \t{.x\n     .y}\nplain\n",
            "<blockquote>\n\n</blockquote>\n<p>plain</p>",
        ),
        (
            "- > {.x\n     .y}\nplain\n",
            "<ul>\n  <li>\n    <blockquote>\n\n    </blockquote>\n  </li>\n</ul>\n<p>plain</p>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(to_html(source), expected, "{source:?}");
    }
}

#[test]
fn continuation_at_the_opener_column_stays_paragraph_text() {
    assert_eq!(
        to_html("> {.x\n  .y}\nplain\n"),
        "<blockquote><p>{.x\n.y}\nplain</p></blockquote>"
    );
}
