//! A `%%%` span leaves a line block's stanza as it stood, so a line below the
//! nested item after an empty stanza is not lazy verse: it ends the span, the
//! line block and the nested item, and the outer item holds it (PART 0 S1,
//! CARVE-P9-047, carve-rs#3804). Expected HTML is the spec model's at carve `5f8be22b`.

fn flat(source: &str) -> String {
    carve::to_html(source)
        .split('\n')
        .map(str::trim)
        .collect::<String>()
}

#[test]
fn an_empty_spanned_stanza_takes_no_lazy_line() {
    let cases = [
        (
            "- a\n  - ::: |\n    %%%\n  c\n",
            "<ul><li>a<ul><li><div class=\"line-block\"></div></li></ul>c</li></ul>",
        ),
        (
            "- a\n  - ::: |\n    %%%\n    %%%\n   c\n",
            "<ul><li>a<ul><li><div class=\"line-block\"></div></li></ul>c</li></ul>",
        ),
        (
            "> - a\n>   - ::: |\n>     %%%\n>     - n\n>    c\n- tail\n",
            "<blockquote><ul><li>a<ul><li><div class=\"line-block\"></div></li></ul>c</li></ul></blockquote><ul><li>tail</li></ul>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn a_stanza_with_verse_still_takes_the_lazy_line() {
    assert_eq!(
        flat("- a\n  - ::: |\n    x\n    %%%\n  c\n"),
        "<ul><li>a<ul><li><div class=\"line-block\"><p>x<br>c</p></div></li></ul></li></ul>"
    );
}
