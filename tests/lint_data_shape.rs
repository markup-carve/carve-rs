use carve::lint::{LintOptions, lint};

#[test]
fn data_shape_uses_the_attribute_grammar() {
    // This integration test has one test process, so the internal switch is isolated.
    unsafe { std::env::set_var("CARVE_LINT_RULES", "data") };
    let options = LintOptions::default();
    for source in ["{}\n\n", "{^}\n\n"] {
        let findings = lint(source, &options);
        assert!(
            findings
                .iter()
                .all(|finding| finding.code != "unattached-block-attribute"),
            "invalid attribute block: {source:?}"
        );
    }
    let findings = lint("{hello}\n\n", &options);
    assert!(findings.iter().any(|finding| {
        finding.code == "unattached-block-attribute"
            && finding
                .pos
                .is_some_and(|pos| pos.start_offset == 0 && pos.end_offset == 7)
    }));
    let nested = "::: note\n:::: warning\n::: box\n%%x\n::::\n:::\n";
    let colons: Vec<_> = lint(nested, &options)
        .into_iter()
        .filter(|finding| finding.code == "colon-fence-length-mismatch")
        .filter_map(|finding| finding.pos.map(|pos| (pos.start_offset, pos.end_offset)))
        .collect();
    assert_eq!(colons, [(34, 38), (39, 42)]);

    let attributed_quote = "{#id}\n> > >::: references\n";
    assert!(
        lint(attributed_quote, &options)
            .iter()
            .all(|finding| finding.code != "block-opener-overindented")
    );

    let attributed_list = "- |= A |= B |\n  | 1 | 2 |\n\n  {.c}\n\ti) : a {=mark=}\n\t\n    +|\n";
    assert!(
        lint(attributed_list, &options)
            .iter()
            .all(|finding| finding.code != "list-item-block-overindented")
    );
}
