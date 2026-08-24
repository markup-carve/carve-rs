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
    assert_eq!(findings[0].rule, "a11y/heading-jump");
    assert!(findings[0].message.contains("1 to 3"));
}

#[test]
fn accepts_structurally_ordered_headings_and_described_images() {
    assert!(lint_accessibility("# One\n\n## Two\n\n![Map](/map.png)\n").is_empty());
}
