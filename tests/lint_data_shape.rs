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
}
