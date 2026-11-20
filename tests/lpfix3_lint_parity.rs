use carve::lint::{LintOptions, lint};

#[test]
fn catalog_line_rules_follow_parsed_ownership() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("data/lint-cases.json")).unwrap();
    for case in cases
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["id"].as_str().is_some_and(|id| id.contains('#')))
    {
        let source = case["input"].as_str().unwrap();
        let code = case["code"].as_str().unwrap();
        let expected = case["reported"].as_bool().unwrap();
        let found = lint(source, &LintOptions::default())
            .iter()
            .any(|finding| finding.code == code);
        assert_eq!(found, expected, "{code} on {source:?}");
    }
}

#[test]
fn quoted_term_and_fence_ranges_and_attribute_message() {
    let cases = [
        ("> :: w\n> w\n", "description-term-multi-line", 9, 10),
        ("> ```\n>   ```\n", "fence-closer-indented", 10, 13),
        (
            "> %%%%\n> %%%% x\n",
            "comment-fence-closer-with-text",
            7,
            15,
        ),
    ];
    for (source, code, from, to) in cases {
        let findings = lint(source, &LintOptions::default());
        let finding = findings
            .iter()
            .find(|finding| finding.code == code)
            .unwrap();
        let position = finding.pos.unwrap();
        assert_eq!(
            (position.start_offset, position.end_offset),
            (from, to),
            "{code}"
        );
    }
    let findings = lint("{#id .c k=v}\n{#id .c k=v}\n\n", &LintOptions::default());
    let finding = findings
        .iter()
        .find(|finding| finding.code == "unattached-block-attribute")
        .unwrap();
    assert!(finding.message.starts_with("A blank line follows"));
}
