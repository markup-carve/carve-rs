fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_comment_inside_a_quote_retains_the_note_body() {
    for comment in ["%% c", "%%%"] {
        let source = format!("> [^c]: para\n>  {comment}\n>   tail\n");
        assert_eq!(html(&source), "<blockquote></blockquote>", "{source:?}");
    }
}
