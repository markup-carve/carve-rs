#[test]
fn native_escapes_and_writer_round_trips() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/image-alt-escapes.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let expected = case["html"].as_str().unwrap();
        assert_eq!(
            carve::to_html(source)
                .trim()
                .replace("&#39;", "&#039;")
                .replace("&apos;", "&#039;"),
            expected,
            "{}",
            case["name"]
        );
        let table = case["table"].as_str().unwrap();
        let image = expected
            .strip_prefix("<p>x ")
            .unwrap()
            .strip_suffix(" y</p>")
            .unwrap();
        assert!(
            carve::to_html(table)
                .replace("&apos;", "&#039;")
                .replace("&#39;", "&#039;")
                .contains(image),
            "table {}",
            case["name"]
        );
        assert!(
            carve::to_html(&carve::to_carve(table))
                .replace("&apos;", "&#039;")
                .replace("&#39;", "&#039;")
                .contains(image),
            "table writer {}",
            case["name"]
        );
        let written = carve::to_carve(source);
        assert_eq!(
            carve::to_html(&written)
                .trim()
                .replace("&#39;", "&#039;")
                .replace("&apos;", "&#039;"),
            expected,
            "{}",
            case["name"]
        );
        assert_eq!(carve::to_carve(&written), written, "{}", case["name"]);
    }
}

#[test]
fn table_image_stays_native() {
    assert!(
        carve::to_html("| ![a\\|b](/i) |\n|---|\n| c |\n").contains("<img src=\"/i\" alt=\"a|b\">")
    );
}

#[test]
fn unresolved_image_stays_literal() {
    assert!(carve::to_html("x ![a\\|b][missing] y").contains("![a\\|b][missing]"));
}

#[test]
fn migrate_escaped_image_alt() {
    let source = "x ![a\\|b][R] y\n\n[r]: /i\n";
    assert_eq!(
        carve::migrate_case_only_references(source, &carve::Options::default()),
        "x ![a\\|b][r] y\n\n[r]: /i\n"
    );
}

#[test]
fn escaped_backtick_beside_code_cell() {
    let source = "| ![a\\`b](/i \"c\\`d\\|e\") | `x` |\n|---|---|\n";
    let expected = carve::to_html(source);
    assert!(expected.contains("alt=\"a`b\" title=\"c`d|e\""));
    assert!(expected.contains("<code>x</code>"));
    assert_eq!(carve::to_html(&carve::to_carve(source)), expected);
}

#[test]
fn migration_uses_the_balanced_alt_boundary() {
    let source = "x ![a\\|b `][]`][R]{title=\"] [R]\"} y\n\n[r]: /i\n";
    assert_eq!(
        carve::migrate_case_only_references(source, &carve::Options::default()),
        source.replace("`][R]", "`][r]")
    );
}
#[test]
fn imported_table_footnotes_keep_references_and_avoid_collisions() {
    let md = "| Note[^a\\|b] | c |\n|---|---|\n\nText[^a|b].\n\n`[^a|b]`\n\n[^a|b]: Note.\n\n[^carve-import-footnote-1]: Existing.\n";
    let written = carve::markdown_to_carve(md);
    assert_eq!(written.matches("[^carve-import-footnote-2]").count(), 3);
    assert!(written.contains("`[^a|b]`"));
    assert!(written.contains("[^carve-import-footnote-1]: Existing."));
    let html = carve::to_html(&written);
    assert_eq!(html.matches("role=\"doc-noteref\"").count(), 2);
    assert!(html.contains("<code>[^a|b]</code>"));
    assert!(html.contains("Note.<a href=\"#fnref1\""));
}
#[test]
fn imported_footnotes_keep_their_identity_and_opaque_content() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/markdown-table-footnotes.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let html = carve::to_html(&carve::markdown_to_carve(case["md"].as_str().unwrap()));
        assert_eq!(
            html.matches("role=\"doc-noteref\"").count(),
            case["refs"].as_u64().unwrap() as usize,
            "{}: {}",
            case["name"],
            html
        );
        assert!(
            html.contains(case["html"].as_str().unwrap()),
            "{}: {}",
            case["name"],
            html
        );
    }
}

#[test]
fn table_pipe_imports_and_round_trips() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/table-pipe-audit.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let input = case["source"].as_str().unwrap();
        let source = if case["mode"] == "md" {
            carve::markdown_to_carve(input)
        } else if case["mode"] == "native-export" {
            carve::markdown_to_carve(&carve::to_markdown(input))
        } else {
            carve::migrate_djot(input).value
        };
        let html = carve::to_html(&source);
        for fragment in case["contains"].as_array().unwrap() {
            assert!(
                html.contains(fragment.as_str().unwrap()),
                "{} {}: {}",
                case["mode"],
                case["name"],
                html
            );
        }
        if let Some(excludes) = case["excludes"].as_array() {
            for fragment in excludes {
                assert!(
                    !html.contains(fragment.as_str().unwrap()),
                    "{}: {}",
                    case["name"],
                    html
                );
            }
        }
        if let Some(cells) = case["cells"].as_u64() {
            assert_eq!(
                html.matches("<th ").count(),
                cells as usize,
                "{}",
                case["name"]
            );
        }
        if let Some(tables) = case["tables"].as_u64() {
            assert_eq!(
                html.matches("<table>").count(),
                tables as usize,
                "{}",
                case["name"]
            );
        }
        assert_eq!(
            carve::to_html(&carve::to_carve(&source)),
            html,
            "{}",
            case["name"]
        );
    }
}
