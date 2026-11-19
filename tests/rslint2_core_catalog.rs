use carve::lint::{LintOptions, lint};

fn matching(source: &str, code: &str) -> Vec<carve::diagnostic::Diagnostic> {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == code)
        .collect()
}

#[test]
fn term_at_container_content_column_is_not_overindented() {
    for source in [
        "> ::: glossary\n> :: a\n> :::",
        "- x\n\n  ::: glossary\n  :: a\n  :::",
    ] {
        assert!(
            matching(source, "block-opener-overindented").is_empty(),
            "{source:?}"
        );
    }
}

#[test]
fn extra_indent_in_div_remains_reportable() {
    for (source, offset) in [
        ("::: glossary\n  :: a\n:::", 15),
        ("::: note\n  ::: references\n  x\n  :::\n:::", 11),
    ] {
        let findings = matching(source, "block-opener-overindented");
        assert_eq!(findings.len(), 1, "{source:?}");
        assert_eq!(findings[0].pos.expect("source range").start_offset, offset);
    }
}

#[test]
fn references_placement_is_void_in_core_lint() {
    let source = "> ::: references\n> :::";
    assert!(matching(source, "references-placement-in-container").is_empty());
}
