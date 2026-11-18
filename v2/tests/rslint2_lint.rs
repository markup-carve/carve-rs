//! C2-T-005: lint includes parse-channel diagnostics from the same document.

use carve2::lint::{LintOptions, lint};

#[test]
fn cc_477_lint_reports_an_unclosed_fence_from_parse() {
    let diagnostics = lint("a **b** c\n\n```\nx\n", &LintOptions::default());
    let finding = diagnostics
        .iter()
        .find(|d| d.code == "unclosed-code-fence")
        .expect("parse finding in lint output");
    assert_eq!(finding.channel, "parse");
    let pos = finding.pos.unwrap();
    assert_eq!((pos.start_line, pos.start_column), (3, 1));
}

#[test]
fn cc_1118_lint_reports_fence_title_parse_diagnostic() {
    let diagnostics = lint("```  {.c}\nx\n```\n", &LintOptions::default());
    assert!(diagnostics.iter().any(|d| d.code == "fence-title-syntax"));
}
