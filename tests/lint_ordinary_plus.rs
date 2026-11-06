use carve::{lint_carve, to_html};

#[test]
fn plus_at_item_content_column_is_text_and_lint_quiet() {
    let source = "- a\n  +\n  x\n";
    assert_eq!(
        to_html(source),
        "<ul>\n  <li>a\n+\nx</li>\n</ul>"
    );
    assert!(lint_carve(source).is_empty(), "{:?}", lint_carve(source));
}

#[test]
fn ordinary_item_plus_variants_remain_quiet() {
    for n in 0..1000 {
        let word = format!("word{n}");
        let source = format!("- {word}\n  +\n  tail{n}\n");
        let findings: Vec<_> = lint_carve(&source)
            .into_iter()
            .filter(|warning| warning.rule == "continuation-marker-refused")
            .collect();
        assert!(findings.is_empty(), "variant {n}: {findings:?}");
    }
}
