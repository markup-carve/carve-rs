use carve::{lint_carve, to_html, to_html_with_options, Options, Tabs};

#[test]
fn bare_tab_titles_recover_without_naming_the_tabs() {
    let source =
        ":::: tabs\n::: tab Install\nBody one.\n:::\n::: tab Configure\nBody two.\n:::\n::::\n";
    let tabs = Tabs::new();
    let html = to_html_with_options(source, &Options::new().with_extension(&tabs));
    assert!(html.contains("class=\"tabs-label\">Tab 1</label>"));
    assert!(html.contains("class=\"tabs-label\">Tab 2</label>"));
    assert!(html.contains("<p>Body one.</p>"));
    assert!(!html.contains(":::"));
    let lines: Vec<_> = lint_carve(source)
        .into_iter()
        .filter(|w| w.rule == "fence-title-syntax")
        .map(|w| w.line)
        .collect();
    assert_eq!(lines, vec![2, 5]);
}

#[test]
fn malformed_metadata_keeps_nested_blocks_and_figure_containers() {
    for metadata in [
        "Bare title",
        "“Curly title”",
        "\"unclosed",
        "[unclosed",
        "\"Good\" [broken",
        "\t\"Tabbed\"",
        "{.inline}",
    ] {
        let source = format!(
            ":::: outer\n::: figure {metadata}\n# Heading\n\n- one\n- two\n:::\n::::\nAfter.\n"
        );
        let html = to_html(&source);
        assert!(
            html.contains("<div class=\"figure\">"),
            "{metadata}: {html}"
        );
        assert!(html.contains("<h1"));
        assert!(html.contains("<ul>"));
        assert!(!html.contains("<figure"));
        assert!(!html.contains(":::"));
        assert_eq!(
            lint_carve(&source)
                .iter()
                .filter(|w| w.rule == "fence-title-syntax")
                .count(),
            1
        );
    }
}

#[test]
fn opaque_payloads_and_glued_kinds_do_not_recover() {
    for source in [
        "```\n::: tab Wrong\n```\n",
        "%%%\n::: tab Wrong\n%%%\n",
        ":::tab Wrong\nbody\n",
    ] {
        assert!(lint_carve(source)
            .iter()
            .all(|w| w.rule != "fence-title-syntax"));
    }
}

#[test]
fn unicode_metadata_separators_are_diagnosed_not_accepted_as_padding() {
    for ws in ['\u{85}', '\u{feff}'] {
        let source = format!("::: note{ws}\"Title\"\nx\n:::\n");
        assert!(to_html(&source).contains("<aside"));
        assert!(!to_html(&source).contains("admonition-title"));
        assert_eq!(
            lint_carve(&source)
                .iter()
                .filter(|w| w.rule == "fence-title-syntax")
                .count(),
            1
        );
    }
}

#[test]
fn bom_and_crlf_diagnostic_offsets() {
    let source = "\u{feff}::: widget Wrong😀\r\nbody\r\n:::\r\n";
    let warnings: Vec<_> = lint_carve(source)
        .into_iter()
        .filter(|w| w.rule == "fence-title-syntax")
        .collect();
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        &source[warnings[0].start..warnings[0].end],
        "::: widget Wrong😀"
    );
    assert_eq!((warnings[0].line, warnings[0].column), (1, 2));
}

#[test]
fn formatting_requires_review_before_dropping_metadata() {
    let patch = carve::to_carve_patch("::: note Wrong\nbody\n:::\n");
    assert!(patch.edits.is_empty());
    assert_eq!(patch.unresolved[0].code, "invalid-container-metadata");
}

#[test]
fn djot_migration_keeps_rejected_opener_text() {
    let html = to_html(&carve::djot_to_carve("::: tip Custom Title\nbody\n:::\n"));
    assert_eq!(html, "<p>::: tip Custom Title\nbody\n:::</p>");
}

#[test]
fn prose_opener_does_not_block_formatting() {
    let patch = carve::to_carve_patch("  ::: widget Bad\nx\n:::\n");
    assert!(!patch.edits.is_empty());
    assert!(patch.unresolved.is_empty());
}

#[test]
fn authored_metadata_and_nested_valid_fences_during_djot_migration() {
    for (metadata, visible) in [("\"T\" extra", " “T” extra"), ("“T”", " “T”"), ("{.x}", "")]
    {
        let source = format!("::: tip {metadata}\nbody\n:::\n");
        assert_eq!(
            to_html(&carve::djot_to_carve(&source)),
            format!("<p>::: tip{visible}\nbody\n:::</p>")
        );
    }
    let source = "::: tip Bad X\n\n::: note\nx\n:::\n:::\n";
    assert_eq!(to_html(&carve::djot_to_carve(source)), "<p>::: tip Bad X</p>\n<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>x</p>\n</aside>\n<p>:::</p>");
}

#[test]
fn line_and_hardbreak_fences_keep_their_own_migrated_closer() {
    for opener in ["::: |", "::: \\"] {
        let source = format!("::: tip Bad X\n{opener}\nl\n:::\nout\n:::\n");
        assert_eq!(
            carve::djot_to_carve(&source),
            format!("\\::: tip Bad X\n{opener}\nl\n:::\nout\n\\:::\n")
        );
    }
}

#[test]
fn footnote_marker_opener_is_diagnosed_and_formatting_requires_review() {
    let source = "a[^1]\n\n[^1]: ::: tip Bad X\n    body\n    :::\n";
    let warnings: Vec<_> = lint_carve(source)
        .into_iter()
        .filter(|w| w.rule == "fence-title-syntax")
        .collect();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 3);
    assert_eq!(&source[warnings[0].start..warnings[0].end], "::: tip Bad X");
    assert_eq!(
        carve::to_carve_patch(source).unresolved[0].code,
        "invalid-container-metadata"
    );
}

#[test]
fn migrated_recovery_does_not_escape_code_payloads() {
    let source = "```\n::: tip Bad X\n:::\n```\n";
    assert_eq!(carve::djot_to_carve(source), source);
}
