//! An empty attribute block after a marker is text (C2-I-040), so it gives no
//! `emphasis-marker-before-attributes`.
use carve::lint::{LintOptions, lint};

fn codes(source: &str) -> Vec<&'static str> {
    lint(source, &LintOptions::default())
        .into_iter()
        .map(|finding| finding.code)
        .collect()
}

#[test]
fn an_empty_block_after_a_marker_is_not_reported() {
    for source in ["~{} a\n", "a *{} b\n", "a _{ } b\n", "a ={}\n", "|~{} |\n"] {
        assert!(
            !codes(source).contains(&"emphasis-marker-before-attributes"),
            "{source:?}"
        );
    }
}

#[test]
fn a_block_with_a_part_is_still_reported() {
    for source in ["a ~{.c} b\n", "a _{.a}{} b\n", "a *{#x} b\n"] {
        assert!(
            codes(source).contains(&"emphasis-marker-before-attributes"),
            "{source:?}"
        );
    }
}
