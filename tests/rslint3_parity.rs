use carve::lint::{LintOptions, lint};

fn has_overindented_opener(source: &str) -> bool {
    lint(source, &LintOptions::default())
        .iter()
        .any(|finding| finding.code == "block-opener-overindented")
}

#[test]
fn description_openers_use_the_description_content_column() {
    for opener in [
        "::: references\n  :::",
        "# Heading",
        "> quote",
        "```\n  ```",
    ] {
        let at_content = format!(":: term\n: body\n  {opener}\n");
        assert!(!has_overindented_opener(&at_content), "{at_content:?}");
    }
    let past_content = ":: term\n: body\n   ::: references\n   :::\n";
    assert!(has_overindented_opener(past_content));
    let quoted = "> :: term\n> : body\n>   ::: references\n>   :::\n";
    assert!(!has_overindented_opener(quoted));
}
