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
    assert!(messages(" | w | w |\n ^ w\n  [^w]: %%\n", code, &options).is_empty());
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
    let inline = "Text ^[note].\n\n[x](#fn1) [y](#fnref1)\n";
    assert!(messages(inline, "broken-fragment-link", &LintOptions::default()).is_empty());
    let admonition = "::: note \"Title\"\ntext\n:::\n\n[x](#adm-1)\n";
    assert!(messages(admonition, "broken-fragment-link", &LintOptions::default()).is_empty());
    let collision = "{#adm-1}\n# H\n\n::: note \"Title\"\ntext\n:::\n\n[x](#adm-1-2)\n";
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
    let normalization = "{id=\"Café\"}\n# H\n\n[x](#Cafe\u{301})\n";
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
    let image = "# ![Alt](image.png)\n\n[x][ALT]\n";
    let findings = messages(image, "unresolved-reference-link", &LintOptions::default());
    assert_eq!(findings.len(), 1);
    assert!(
        findings[0].contains("the label Alt differs only in case"),
        "{findings:?}"
    );
}

#[test]
fn djot_pair_scan_does_not_reuse_a_closing_mark() {
    let mut options = LintOptions::default();
    options
        .rules
        .insert("djot-subscript-tilde".to_string(), "on".to_string());
    assert_eq!(
        messages("~a~b~\n", "djot-subscript-tilde", &options).len(),
        1
    );
    assert!(messages("\\~a~\n", "djot-subscript-tilde", &options).is_empty());
}

#[test]
fn a_dropped_raw_block_id_is_no_case_only_target() {
    let source =
        "{#Raw}\n```=latex\nx\n```\n\n{#Hh}\n```=html\n<b>x</b>\n```\n\n[a](#raw) [b](#hh)\n";
    let found = messages(source, "broken-fragment-link", &LintOptions::default());
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found[0].contains("matches no id in this document"),
        "{found:?}"
    );
    assert!(
        found[1].contains("the id \"Hh\" differs only in case"),
        "{found:?}"
    );
}
