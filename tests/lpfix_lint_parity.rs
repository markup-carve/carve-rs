use carve::lint::{LintOptions, lint};

fn spans(source: &str, rule: &str) -> Vec<(u32, u32)> {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == rule)
        .filter_map(|finding| finding.pos)
        .map(|pos| (pos.start_offset, pos.end_offset))
        .collect()
}

#[test]
fn indented_code_requires_an_indented_paragraph() {
    for source in ["\n      {{ x.crv }}\n", "\n    a. x\n", "\n      = H\n"] {
        assert!(
            spans(source, "indented-code-block").is_empty(),
            "{source:?}"
        );
    }
    assert_eq!(spans("\n    plain text\n", "indented-code-block"), vec![(5, 15)]);
}

#[test]
fn separator_row_allows_trailing_whitespace() {
    assert_eq!(
        spans("| A | B |\n|---|---| \n", "deprecated-separator-row"),
        vec![(10, 20)]
    );
    assert_eq!(
        spans("| A | B |\n|---|---|\t\n", "deprecated-separator-row"),
        vec![(10, 20)]
    );
}

#[test]
fn block_lint_follows_the_parsed_leaf() {
    for source in ["a\n# H #\n", "\t:: t\n: ## H #\n"] {
        assert!(
            spans(source, "heading-closing-hashes").is_empty(),
            "{source:?}"
        );
    }
    assert_eq!(spans("== H #\n", "heading-closing-hashes"), vec![(5, 6)]);
    assert!(
        spans("{{ x.crv#s }}\n-----\n", "setext-heading-underline").is_empty()
    );
    assert_eq!(
        spans("paragraph\n-----\n", "setext-heading-underline"),
        vec![(10, 15)]
    );
}
