//! A collapsed `[text][]` reaches a heading by its exact text only (PART 11
//! R1, `CARVE-P9R-010`). Heading-id transforms shape ids, never this lookup.

use carve::{AsciiHeadingIds, Options};

fn lowercase() -> Options<'static> {
    Options::new().with_lowercase_heading_ids(true)
}

fn ascii(mode: AsciiHeadingIds) -> Options<'static> {
    Options::new().with_ascii_heading_ids(mode)
}

fn assert_literal(source: &str, options: &Options<'_>) {
    let html = carve::to_html_with_options(source, options);
    assert!(!html.contains("<a "), "resolved: {html}");
    let warnings = carve::lint_carve_with_options(source, options);
    assert!(
        warnings
            .iter()
            .any(|w| w.rule == "unresolved-reference-link"),
        "no unresolved-reference-link: {warnings:?}"
    );
}

#[test]
fn lowercase_ids_do_not_fold_the_label() {
    for label in ["plan", "PLAN"] {
        assert_literal(&format!("# Plan\n\nsee [{label}][]\n"), &lowercase());
    }
    assert_literal("# Über uns\n\nsee [über uns][]\n", &lowercase());
}

#[test]
fn ascii_ids_do_not_transliterate_the_label() {
    for mode in [AsciiHeadingIds::Fold, AsciiHeadingIds::Strict] {
        assert_literal("# Über uns\n\nsee [Uber uns][]\n", &ascii(mode));
    }
}

#[test]
fn lowercase_and_ascii_together_do_not_reach_the_heading_by_its_slug() {
    let options = lowercase().with_ascii_heading_ids(AsciiHeadingIds::Fold);
    assert_literal("# Über uns\n\nsee [uber-uns][]\n", &options);
}

#[test]
fn an_explicit_id_is_not_a_collapsed_reference_key() {
    let source = "{#Getting-Started}\n# Intro\n\nsee [Getting Started][]\n";
    assert_literal(source, &Options::default());
    assert_literal(source, &ascii(AsciiHeadingIds::Fold));
}

#[test]
fn the_exact_text_still_resolves_under_every_transform() {
    for options in [
        Options::default(),
        lowercase(),
        ascii(AsciiHeadingIds::Fold),
        lowercase().with_ascii_heading_ids(AsciiHeadingIds::Fold),
    ] {
        let html = carve::to_html_with_options("# Über uns\n\nsee [Über uns][]\n", &options);
        assert!(html.contains("<a href=\"#"), "{html}");
        let warnings = carve::lint_carve_with_options("# Über uns\n\nsee [Über uns][]\n", &options);
        assert!(
            !warnings
                .iter()
                .any(|w| w.rule == "unresolved-reference-link"),
            "{warnings:?}"
        );
    }
}
