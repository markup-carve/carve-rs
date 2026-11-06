use carve::lint_carve;

#[test]
fn list_item_opener_uses_only_specific_overindent_rule() {
    let source = "- ::: box\n   ::: note\n   x\n   :::\n  :::\n";
    let rules: Vec<_> = lint_carve(source)
        .into_iter()
        .filter(|warning| warning.line == 2)
        .map(|warning| (warning.column, warning.rule))
        .collect();
    assert_eq!(rules, vec![(4, "list-item-block-overindented")]);
}

#[test]
fn hosted_opener_variants_emit_one_rule() {
    for n in 0..1000 {
        let source = format!("- ::: box\n   ::: note\n   word{n}\n   :::\n  :::\n");
        let rules: Vec<_> = lint_carve(&source)
            .into_iter()
            .filter(|warning| warning.line == 2)
            .map(|warning| warning.rule)
            .collect();
        assert_eq!(rules, vec!["list-item-block-overindented"], "variant {n}");
    }
}
