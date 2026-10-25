#[test]
fn unmatched_closing_parentheses_keep_their_destination() {
    for destination in ["/a)b", "/a]b)c%25", "/a)b)c"] {
        let source = format!("[x]({})\n", destination.replace(')', "\\)"));
        let markdown = carve::to_markdown(&source);
        assert!(markdown.contains("\\)"), "{markdown}");
        assert_eq!(carve::to_html(&source), carve::to_html(&carve::migrate_markdown(&markdown).value));
    }
}
