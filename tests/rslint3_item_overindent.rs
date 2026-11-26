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

#[test]
fn opener_inside_div_in_item_uses_item_content_column() {
    let source = "1. ::::\n    >\n";
    let findings = lint(source, &LintOptions::default());
    let item: Vec<_> = findings
        .iter()
        .filter(|finding| finding.code == "list-item-block-overindented")
        .collect();
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].pos.unwrap().start_offset, 12);
    assert_eq!(item[0].pos.unwrap().end_offset, 13);
    assert!(
        !findings
            .iter()
            .any(|finding| finding.code == "block-opener-overindented")
    );
}
