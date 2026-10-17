#[test]
fn an_unclosed_comment_fence_keeps_a_code_fence_in_the_note() {
    for source in [
        "[^c]: para\n %%%\n```\n",
        "[^c]: - para\n %%%\n```\n",
        "[^c]: > para\n %%%\n```\n",
    ] {
        assert_eq!(carve::to_html(source), "", "{source:?}");
    }
}
