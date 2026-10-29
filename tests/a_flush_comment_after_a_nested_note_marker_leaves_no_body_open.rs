fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_flush_comment_after_a_nested_note_marker_leaves_no_body_open() {
    assert_eq!(html("[^c]: [^d]: para\n%%%\n        tail\n"), "<p>tail</p>");
}

#[test]
fn a_flush_comment_retains_an_open_term_in_a_note() {
    assert_eq!(html("[^c]: :: t\n%% c\nx\n"), "<p>x</p>");
}
