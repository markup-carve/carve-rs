#[test]
fn an_unclosed_indented_verse_comment_hides_the_rest_of_the_host() {
    assert_eq!(
        carve::to_html("::: |\n %%%\n"),
        "<div class=\"line-block\">\n\n</div>"
    );
}

#[test]
fn a_verse_comment_opens_through_three_spaces_but_not_four() {
    for spaces in 0..=3 {
        let source = format!("::: |\n{}%%%\nsecret\n%%%\n:::\n", " ".repeat(spaces));
        assert_eq!(carve::to_html(&source), "<div class=\"line-block\">\n\n</div>");
    }
    assert!(carve::to_html("::: |\n    %%%\n:::\n").contains("%%%"));
}
