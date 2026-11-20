use carve::lint::{LintOptions, lint};

#[test]
fn nested_quotes_keep_lint_records() {
    let source = format!("{}x\n", "> ".repeat(200));
    let actual = lint(&source, &LintOptions::default());
    assert!(actual.is_empty());
}
