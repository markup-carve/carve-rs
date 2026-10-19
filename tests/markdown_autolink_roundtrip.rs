#[test]
fn representable_autolinks_keep_their_angle_spelling() {
    for source in [
        "<https://github.com/markup-carve>",
        "<https://example.com?a=1&b=2>",
        "<https://example.com/café>",
        "<HTTPS://EXAMPLE.COM>",
        "<user@example.com>",
        "<mailto:user@example.com>",
    ] {
        let markdown = carve::to_markdown(source);
        assert_eq!(markdown.trim_end(), source);
        assert_eq!(carve::markdown_to_carve(&markdown).trim_end(), source);
    }
}

#[test]
fn autolinks_fall_back_when_angle_spelling_changes_the_content() {
    for (source, expected) in [
        (
            "<https://example.com?a=1&copy;>",
            "[https://example.com?a=1\\&copy;](https://example.com?a=1&amp;copy;)",
        ),
        (
            "<https://example.com>{title=\"Example\"}",
            "[https://example.com](https://example.com \"Example\")",
        ),
        (
            "<https://example.com/a(b)>",
            "[https://example.com/a(b)](https://example.com/a%28b%29)",
        ),
        ("<javascript:alert1>", "[javascript:alert1]()"),
        ("<user@-x.com>", "[user@-x.com](mailto:user@-x.com)"),
    ] {
        assert_eq!(carve::to_markdown(source).trim_end(), expected, "{source}");
    }
}

#[test]
fn markdown_autolinks_outside_carve_grammar_stay_links() {
    for source in [
        "<https://a.com/a\"b>",
        "<https://a.com/{x}>",
        "<https://a.com/`x`>",
        "<https://a.com/a\\b>",
        "<a!b@x.com>",
        "<a@x.c0m>",
    ] {
        let carve_source = carve::markdown_to_carve(source);
        assert!(
            carve::to_html(&carve_source).contains("<a href="),
            "{source}: {carve_source}"
        );
    }
}

#[test]
fn markdown_autolink_fallback_keeps_literal_destination_brackets() {
    use carve::{migrate_markdown, HtmlImportSeverity, MigrationConfidence, MigrationFidelity};

    let cases = [
        (
            "<https://example.com/\\[\\>\n",
            "[https://example.com/\\\\\\[\\\\](https://example.com/%5C[%5C)\n",
            &["markdown-paragraph", "markdown-autolink"][..],
        ),
        (
            "<https://e.com/a\\b>\n",
            "[https://e.com/a\\\\b](https://e.com/a%5Cb)\n",
            &["markdown-paragraph", "markdown-autolink"],
        ),
        (
            "<https://e.com/a`b>\n",
            "[https://e.com/a\\`b](https://e.com/a%60b)\n",
            &["markdown-paragraph", "markdown-autolink"],
        ),
        (
            "<https://e.com/a[b]>\n",
            "<https://e.com/a[b]>\n",
            &["markdown-paragraph", "markdown-autolink"],
        ),
        (
            "[a](<x[y]>)\n",
            "[a](x[y])\n",
            &["markdown-paragraph", "markdown-link"],
        ),
        (
            "[a](<x y>)\n",
            "[a](x%20y)\n",
            &["markdown-paragraph", "markdown-link"],
        ),
        (
            "see https://example.com/[a]b here\n",
            "see [https://example.com/[a]b](https://example.com/%5Ba%5Db) here\n",
            &["markdown-paragraph", "markdown-bare-autolink"],
        ),
        (
            "https://e.com/a]b\n",
            "[https://e.com/a\\]b](https://e.com/a%5Db)\n",
            &["markdown-paragraph", "markdown-bare-autolink"],
        ),
    ];
    for (source, expected, codes) in cases {
        let result = migrate_markdown(source);
        assert_eq!(result.value, expected, "{source}");
        let actual: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .map(|row| row.code.as_str())
            .collect();
        assert_eq!(actual, codes, "{source}");
        for row in &result.report.diagnostics {
            assert_eq!(row.path.as_deref(), Some("line:1"), "{source}");
            assert_eq!(row.confidence, MigrationConfidence::Exact, "{source}");
            assert_eq!(row.severity, HtmlImportSeverity::Info, "{source}");
            assert_eq!(
                row.fidelity,
                if matches!(row.code.as_str(), "markdown-paragraph" | "markdown-link") {
                    MigrationFidelity::Preserved
                } else {
                    MigrationFidelity::Normalized
                },
                "{source}"
            );
        }
    }
}

#[test]
fn markdown_autolink_fallback_keeps_table_pipe_encoded() {
    let source = "| URL |\n| --- |\n| <https://e.com/a\\|b> |\n";
    let result = carve::migrate_markdown(source);
    assert!(result.value.contains("%7C"), "{}", result.value);
}

#[test]
fn markdown_bracketed_autolink_keeps_its_angle_form() {
    for source in [
        "[foo<https://example.com/?search=](uri)>\n",
        "a [b <https://e.com/]> c\n",
        "<https://e.com/a[b> c](/u)\n",
    ] {
        assert_eq!(carve::migrate_markdown(source).value, source);
    }
}
