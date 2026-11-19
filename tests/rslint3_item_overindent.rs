use carve::lint::{LintOptions, lint};

fn count(source: &str) -> usize {
    lint(source, &LintOptions::default())
        .iter()
        .filter(|finding| finding.code == "list-item-block-overindented")
        .count()
}

#[test]
fn later_item_block_openers_past_the_content_column_are_reported() {
    for opener in ["::: note\n  :::", "::: box\n  :::"] {
        let overindented = format!("- a\n    {opener}\n");
        assert_eq!(count(&overindented), 1, "{overindented:?}");
        let at_content = format!("- a\n  {opener}\n");
        assert_eq!(count(&at_content), 0, "{at_content:?}");
    }
    assert_eq!(count("- ::: note\n  :::\n"), 0);
}
