//! Catalog row regressions for the next2 lint model and hand rules.
use carve::lint::{LintOptions, lint};

fn messages(source: &str, code: &str, options: &LintOptions) -> Vec<String> {
    lint(source, options)
        .into_iter()
        .filter(|finding| finding.code == code)
        .map(|finding| finding.message)
        .collect()
}

#[test]
fn bare_nested_quote_after_paragraph_is_an_opener_candidate() {
    let code = "block-opener-without-blank-line";
    assert_eq!(
        messages("> para\n> >\n", code, &LintOptions::default()).len(),
        1
    );
    assert!(messages("> para\n>\n> >\n", code, &LintOptions::default()).is_empty());
}

#[test]
fn indented_caption_continuation_is_not_a_definition() {
    let code = "definition-in-container";
    let options = LintOptions::default();
    assert!(messages(" | w | w |\n ^ w\n [^w]: %%\n", code, &options).is_empty());
    assert_eq!(messages("> [^w]: note\n", code, &options).len(), 1);
}

#[test]
fn numbered_caption_ids_are_crossref_candidates() {
    let source = "{#tab}\n| a |\n^ #: Cap\n\nSee </#TAB>.\n";
    let findings = messages(source, "broken-crossref", &LintOptions::default());
    assert_eq!(findings.len(), 1);
    assert!(
        findings[0].contains("the id tab differs only in case"),
        "{findings:?}"
    );
}

#[test]
fn rendered_footnotes_and_raw_html_anchors_are_fragment_targets() {
    let options = LintOptions::default();
    let footnotes = "Text[^n].\n\n[^n]: note\n\n[x](#fn1) [y](#fnref1)\n";
    assert!(messages(footnotes, "broken-fragment-link", &options).is_empty());
    let mut raw_options = options;
    raw_options.raw_html = true;
    let raw = "```=html\n<a id=\"raw\">x</a>\n```\n\n[x](#raw)\n";
    assert!(messages(raw, "broken-fragment-link", &raw_options).is_empty());
    assert_eq!(
        messages(raw, "broken-fragment-link", &LintOptions::default()).len(),
        1
    );
    let collision = "{#fn1}\n# H\n\nText[^n].\n\n[^n]: note\n\n[x](#fn1-2)\n";
    assert!(messages(collision, "broken-fragment-link", &LintOptions::default()).is_empty());
}

#[test]
fn reference_near_misses_use_heading_text_and_unicode_fold() {
    let source = "{#foo}\n# Heading\n\n[x][HEADING] [y][FOO]\n";
    let findings = messages(source, "unresolved-reference-link", &LintOptions::default());
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(
        findings[0].contains("the label Heading differs only in case"),
        "{findings:?}"
    );
    assert!(
        !findings[1].contains("differs only in case"),
        "{findings:?}"
    );
    let unicode = "{id=\"Straße\"}\n# H\n\n</#STRASSE>\n";
    let findings = messages(unicode, "broken-crossref", &LintOptions::default());
    assert_eq!(findings.len(), 1);
    assert!(
        findings[0].contains("the id Straße differs only in case"),
        "{findings:?}"
    );
    let spaced = "# A  B\n\n[x][a b]\n";
    let findings = messages(spaced, "unresolved-reference-link", &LintOptions::default());
    assert_eq!(findings.len(), 1);
    assert!(
        findings[0].contains("the label A  B differs only in case"),
        "{findings:?}"
    );
    let normalization = "{id=\"Café\"}\n# H\n\n[x](#Café)\n";
    let findings = messages(
        normalization,
        "broken-fragment-link",
        &LintOptions::default(),
    );
    assert_eq!(findings.len(), 1);
    assert!(
        !findings[0].contains("differs only in case"),
        "{findings:?}"
    );
}
