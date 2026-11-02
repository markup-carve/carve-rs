fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_comment_inside_a_quote_retains_the_note_body() {
    let source = "> [^c]: para\n>  %% c\n>   tail\n";
    assert_eq!(
        html(source),
        "<blockquote><p>tail</p></blockquote>",
        "{source:?}"
    );
    // carve#4907: the unclosed span hides `tail`.
    let source = "> [^c]: para\n>  %%%\n>   tail\n";
    assert_eq!(html(source), "<blockquote></blockquote>", "{source:?}");
}
