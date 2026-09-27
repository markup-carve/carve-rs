use carve::lint_carve;

#[test]
fn source_rules_report_the_shared_triggers() {
    for (rule, source) in [
        ("bidi-control-in-source", "a\u{202e}b\n"),
        ("heading-trailing-attribute", "# Title {#id}\n"),
        (
            "fence-opener-fallback",
            "``` php extra bad info\ncode\n```\n",
        ),
        ("raw-block-syntax", "```raw html\nx\n```\n"),
        ("blockquote-marker-without-space", ">quoted\n"),
        ("block-marker-as-text", "  ::: note\n"),
        ("fence-delimiter-indentation", "  ```\n  x\n  ```\n"),
        ("list-item-body-detached", "1. item\n\n  # heading\n"),
        (
            "list-item-block-overindented",
            "-{.x1} item\n\n       # heading\n",
        ),
        ("empty-include-path", "{{ #section }}\n"),
        (
            "carve-version-unsupported",
            "---\ncarve-version: 99.0\n---\n\nx\n",
        ),
        ("unclosed-container-fence", "::: note\nbody\n"),
        ("colon-fence-length-mismatch", ":::: note\nbody\n:::\n"),
        (
            "figure-group-empty",
            "::: figure\njust a paragraph\n:::\n^ Figure #: G\n",
        ),
        (
            "figure-group-single-panel",
            "::: figure\n![a](a.png)\n^ (a) A\n:::\n^ Figure #: G\n",
        ),
        ("fence-title-syntax", "::: note Some Title\nbody\n:::\n"),
        (
            "footnotes-placement-in-container",
            "Intro[^a].\n\n> ::: footnotes\n> :::\n\n[^a]: only note\n",
        ),
        ("table-cell-attribute-before-marker", "|{#x}< content |\n"),
    ] {
        assert!(
            lint_carve(source).iter().any(|w| w.rule == rule),
            "{rule}: {source:?}"
        );
    }
}

#[test]
fn literal_examples_and_closed_containers_do_not_warn() {
    for source in [
        "```\n{{ #x }}\n>quoted\n  ::: note\n```\n",
        "%%%\n{{ #x }}\n>quoted\n%%%\n",
        "`{{ #x }}`\n",
        "\\{{ #x }}\n",
        "::: note\nbody\n:::\n",
        "> ::: note\n> body\n> :::\n",
        "- item\n\n  # Heading\n",
        "---\ncarve-version: 0.1.0\n---\n\nx\n",
    ] {
        assert!(
            lint_carve(source).is_empty(),
            "{source:?}: {:?}",
            lint_carve(source)
        );
    }
}

#[test]
fn include_warning_spans_use_original_unicode_bytes() {
    let source = "😀\r\n\r\n> {{ #missing }}\r\n";
    let warning = lint_carve(source)
        .into_iter()
        .find(|w| w.rule == "empty-include-path")
        .unwrap();
    assert_eq!(warning.line, 3);
    assert_eq!(warning.column, 3);
    assert_eq!(&source[warning.start..warning.end], "{{ #missing }}");
}

#[test]
fn fence_diagnostics_respect_the_owner_and_closer_column() {
    for source in [
        "::: note\n::: tip\nx\n:::\n",
        "::: note\nx\n\n    :::\n",
        "a. ::: note\n   x\n",
    ] {
        assert!(
            lint_carve(source)
                .iter()
                .any(|w| w.rule == "unclosed-container-fence"),
            "{source:?}"
        );
    }
    assert!(lint_carve(":::: note\n:::\ninner\n:::\n::::\n").is_empty());
}

#[test]
fn cell_marker_advice_requires_a_real_unpadded_cell_prefix() {
    for source in [
        "| a `|{#x}<` b |\n",
        "| a \\|{#x}< b |\n",
        "| {#x}< b |\n",
        "> | a | {#x}< b |\n",
    ] {
        assert!(
            !lint_carve(source)
                .iter()
                .any(|w| w.rule == "table-cell-attribute-before-marker"),
            "{source:?}"
        );
    }
    assert!(lint_carve("> |{#x}< b |\n")
        .iter()
        .any(|w| w.rule == "table-cell-attribute-before-marker"));
}

#[test]
fn quoted_markup_gets_the_specific_warning_and_marker_span() {
    assert!(lint_carve("> ```raw html\n> x\n> ```\n")
        .iter()
        .any(|w| w.rule == "raw-block-syntax"));
    assert!(lint_carve("> # T {#id}\n")
        .iter()
        .any(|w| w.rule == "heading-trailing-attribute"));
    let source = "  {.cls} text\n";
    let warning = lint_carve(source)
        .into_iter()
        .find(|w| w.rule == "block-marker-as-text")
        .unwrap();
    assert_eq!(&source[warning.start..warning.end], "{.");
}
