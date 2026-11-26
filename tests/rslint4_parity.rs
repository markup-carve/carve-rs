use carve::lint::{LintOptions, lint};

fn codes(source: &str) -> Vec<String> {
    // C2-W-156: `thematic-break-spelling` runs under `upgrade`.
    let options = LintOptions {
        upgrade: true,
        ..LintOptions::default()
    };
    lint(source, &options)
        .into_iter()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect()
}

#[test]
fn unclosed_frontmatter_opener_has_no_shape_warning() {
    for source in ["---", "---\n"] {
        let codes = codes(source);
        assert!(
            codes.iter().any(|code| code == "unclosed-frontmatter"),
            "{source:?}: {codes:?}"
        );
        assert!(
            !codes
                .iter()
                .any(|code| code == "frontmatter-shaped-thematic-break"),
            "{source:?}: {codes:?}"
        );
    }
    assert!(
        codes("---\n\n---")
            .iter()
            .any(|code| code == "frontmatter-shaped-thematic-break")
    );
}

#[test]
fn quoted_lazy_heading_reports_its_content_marker() {
    for source in ["> x\n> # H\n", "> > x\n> > # H\n", "- > x\n  > # H\n"] {
        let diagnostics = lint(source, &LintOptions::default());
        let findings: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "block-opener-without-blank-line")
            .collect();
        assert_eq!(findings.len(), 1, "{source:?}: {diagnostics:?}");
        assert!(
            findings[0].message.contains("a heading"),
            "{source:?}: {findings:?}"
        );
    }
}

#[test]
fn quoted_hyphen_run_has_one_contextual_warning() {
    for source in ["> x\n> ---\n", "> > x\n> > ---\n"] {
        let codes = codes(source);
        assert_eq!(
            codes
                .iter()
                .filter(|code| *code == "setext-heading-underline")
                .count(),
            1,
            "{source:?}: {codes:?}"
        );
        assert!(
            !codes.iter().any(|code| code == "thematic-break-spelling"),
            "{source:?}: {codes:?}"
        );
    }
    let codes = codes("> x\n>\n> ---\n");
    assert!(!codes.iter().any(|code| code == "setext-heading-underline"));
    assert!(codes.iter().any(|code| code == "thematic-break-spelling"));
}
