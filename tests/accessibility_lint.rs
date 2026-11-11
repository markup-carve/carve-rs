use carve::{lint_accessibility, AccessibilitySeverity};

#[test]
fn reports_empty_image_alt_with_a_source_range() {
    let findings = lint_accessibility("![](/image.png)\n");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, "a11y/image-alt");
    assert_eq!(findings[0].severity, AccessibilitySeverity::Error);
    assert_eq!(findings[0].start_offset, Some(0));
}

#[test]
fn reports_heading_level_jumps_but_not_descents() {
    let findings = lint_accessibility("# One\n\n### Three\n\n## Two\n");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, "heading-level-skip");
    assert!(findings[0].message.contains("1 to 3"));
}

#[test]
fn heading_jump_rule_is_canonical_across_1000_documents() {
    for variant in 0..1000 {
        let first = variant % 4 + 1;
        let next = first + 2;
        let prefix = if variant % 2 == 0 { "A paragraph.\n\n" } else { "" };
        let between = if variant % 3 == 0 { "\nText between headings.\n" } else { "" };
        let suffix = if variant % 5 == 0 { "\n\n> A quoted sibling.\n" } else { "" };
        let source = format!(
            "{prefix}{} First {variant}\n{between}\n{} Next {variant}\n{suffix}",
            "#".repeat(first),
            "#".repeat(next)
        );
        let findings = lint_accessibility(&source);
        assert_eq!(findings.len(), 1, "variant {variant}: {source}");
        assert_eq!(findings[0].rule, "heading-level-skip", "variant {variant}");
        assert_eq!(findings[0].severity, AccessibilitySeverity::Warning);
    }
}

#[test]
fn accepts_structurally_ordered_headings_and_described_images() {
    assert!(lint_accessibility("# One\n\n## Two\n\n![Map](/map.png)\n").is_empty());
}
