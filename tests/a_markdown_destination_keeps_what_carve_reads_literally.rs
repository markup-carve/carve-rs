use carve::{migrate_markdown, MigrationFidelity};

#[test]
fn ruled_markdown_destinations() {
    let cases: &[(&str, &str, &str, &[&str])] = &[
        (
            "D1",
            r#"[a](/x\y) [b](/a`b) [c](/a^b{c}) [d](/a"b) [e](/a<b>)"#,
            r#"[a](/x\y) [b](/a`b) [c](/a^b{c}) [d](/a"b) [e](/a<b>)"#,
            &[
                "markdown-paragraph",
                "markdown-link",
                "markdown-link",
                "markdown-link",
                "markdown-link",
                "markdown-link",
            ],
        ),
        (
            "D2",
            r"[e](/x\\)",
            r"[e](/x\\)",
            &["markdown-paragraph", "markdown-link"],
        ),
        (
            "D3",
            r"[f](</a\\(>)",
            r"[f](/a\\\()",
            &["markdown-paragraph", "markdown-link"],
        ),
        (
            "D4",
            r"[g](/a\\\\b)",
            r"[g](/a\\\b)",
            &["markdown-paragraph", "markdown-link"],
        ),
        (
            "D5",
            r"[a](</x)(a>) [b](</x(a))>) [c](</x((a)>) [d](</x\\(a)>)",
            r"[a](/x\)\(a) [b](/x(a)\)) [c](/x\((a)) [d](/x\\(a))",
            &[
                "markdown-paragraph",
                "markdown-link",
                "markdown-link",
                "markdown-link",
                "markdown-link",
            ],
        ),
        (
            "D6",
            "[a](/x(a))",
            "[a](/x(a))",
            &["markdown-paragraph", "markdown-link"],
        ),
        (
            "D7",
            "[a][r]\n\n[r]: /x`y",
            "[a][r]\n\n[r]: /x`y",
            &[
                "markdown-paragraph",
                "markdown-reference-link",
                "markdown-reference-definition",
            ],
        ),
        (
            "D8",
            "![a](/p^q.png)",
            "![a](/p^q.png)",
            &["markdown-paragraph", "markdown-image"],
        ),
        (
            "D9",
            "| h |\n| --- |\n| [a](/x\\|y) z |",
            "|= h |\n| [a](/x%7Cy) z |",
            &[
                "markdown-table",
                "markdown-table-row",
                "markdown-table-cell",
                "markdown-table-row",
                "markdown-table-cell",
                "markdown-link",
            ],
        ),
        (
            "A1",
            r"<https://example.com/\[\>",
            r"[https://example.com/\\\[\\](https://example.com/\[\\)",
            &["markdown-paragraph", "markdown-autolink"],
        ),
        (
            "A2",
            r"<https://e.com/a\b>",
            r"[https://e.com/a\\b](https://e.com/a\b)",
            &["markdown-paragraph", "markdown-autolink"],
        ),
        (
            "A3",
            "<https://e.com/a`b>",
            r"[https://e.com/a\`b](https://e.com/a`b)",
            &["markdown-paragraph", "markdown-autolink"],
        ),
        (
            "E1",
            "see https://example.com/[a]b here",
            "see [https://example.com/[a]b](https://example.com/[a]b) here",
            &["markdown-paragraph", "markdown-bare-autolink"],
        ),
        (
            "E2",
            "see https://e.com/ü here",
            "see [https://e.com/ü](https://e.com/ü) here",
            &["markdown-paragraph", "markdown-bare-autolink"],
        ),
        (
            "E3",
            "www.example.com/a[b]c",
            "[www.example.com/a[b]c](http://www.example.com/a[b]c)",
            &["markdown-paragraph", "markdown-bare-autolink"],
        ),
        (
            "E4",
            "https://e.com/a]b",
            r"[https://e.com/a\]b](https://e.com/a]b)",
            &["markdown-paragraph", "markdown-bare-autolink"],
        ),
        (
            "E5",
            "see https://e.com/[x](y) z",
            r"see [https://e.com/[x]\(y)](https://e.com/[x](y)) z",
            &["markdown-paragraph", "markdown-bare-autolink"],
        ),
        (
            "F1",
            "[x <https://e.com/a]b> y](/u)",
            "[x <https://e.com/a]b> y](/u)",
            &[],
        ),
        ("control-entity", "[a](/x&amp;y)", "[a](/x&y)", &[]),
        (
            "control-autolink",
            "<https://e.com/a[b]>",
            "<https://e.com/a[b]>",
            &["markdown-paragraph", "markdown-autolink"],
        ),
        (
            "control-unicode",
            "[a](/ü.png)",
            "[a](/ü.png)",
            &["markdown-paragraph", "markdown-link"],
        ),
    ];
    for &(name, source, expected, codes) in cases {
        let result = migrate_markdown(&format!("{source}\n"));
        assert_eq!(result.value, format!("{expected}\n"), "{name}");
        if matches!(name, "control-entity" | "F1") {
            assert_eq!(result.report.diagnostics[0].code, "fidelity-unverified");
        }
        if !codes.is_empty() {
            let actual: Vec<_> = result
                .report
                .diagnostics
                .iter()
                .map(|row| row.code.as_str())
                .collect();
            assert_eq!(actual, codes, "{name}");
            for row in &result.report.diagnostics {
                if name == "D9" {
                    continue;
                }
                assert_eq!(
                    row.path.as_deref(),
                    Some(
                        if name == "D7" && row.code == "markdown-reference-definition" {
                            "line:3"
                        } else {
                            "line:1"
                        }
                    ),
                    "{name}"
                );
                if name == "D1" {
                    assert_eq!(row.fidelity, MigrationFidelity::Preserved, "{name}");
                }
                if name.starts_with('A') || name.starts_with('E') {
                    assert_eq!(
                        row.fidelity,
                        if row.code == "markdown-paragraph" {
                            MigrationFidelity::Preserved
                        } else {
                            MigrationFidelity::Normalized
                        },
                        "{name}"
                    );
                }
            }
        }
    }
}

#[test]
fn table_email_autolinks_encode_destination_pipes() {
    let cases = [
        (
            r"| <a\|b@x.com> | z |\n|---|---|",
            r"|= [a\|b@x.com](mailto:a%7Cb@x.com) |= z |",
        ),
        (
            r"| <a*\|b*c@x.com> | z |\n|---|---|",
            r"|= [a*\|b*c@x.com](mailto:a*%7Cb*c@x.com) |= z |",
        ),
        (
            r"| <a`\|b`c@x.com> | z |\n|---|---|",
            r"|= [a\`\|b\`c@x.com](mailto:a`%7Cb`c@x.com) |= z |",
        ),
        (
            r"| <\|a@x.com> | z |\n|---|---|",
            r"|= [\|a@x.com](mailto:%7Ca@x.com) |= z |",
        ),
        (
            r"| ][<a\|b@x.com>[] |\n|---|",
            r"|= ]\[[a\|b@x.com](mailto:a%7Cb@x.com)[\] |",
        ),
        (
            r"| [a][<a\|b@x.com>] |\n|---|",
            r"|= [a]\[[a\|b@x.com](mailto:a%7Cb@x.com)\] |",
        ),
    ];
    for (source, expected) in cases {
        let source = source.replace(r"\n", "\n");
        assert_eq!(
            migrate_markdown(&source).value,
            format!("{expected}\n"),
            "{source}"
        );
    }
}

#[test]
fn email_autolink_destination_keeps_backticks() {
    let carve = carve::markdown_to_carve("<a`b@x.com>\n");
    assert_eq!(carve, "[a\\`b@x.com](mailto:a`b@x.com)\n");
}
