#[test]
fn preserves_quote_and_list_ownership() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/djot-quoted-list-boundaries.json")).unwrap();
    let mut failures = Vec::new();
    for case in cases.as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let expected = case["carve"].as_str().unwrap();
        let imported = carve::djot_to_carve(source);
        let actual_html = carve::to_html(&imported);
        let expected_html = carve::to_html(expected);
        let canonical = |html: &str| {
            html.replace("&#10;", "\n")
                .replace("&#62;", "&gt;")
                .replace('\u{a0}', "&nbsp;")
        };
        if canonical(&actual_html) != canonical(&expected_html) {
            failures.push(format!("{source:?}\nImported: {imported:?}\nActual: {actual_html}\nExpected: {expected_html}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn root_fences_release_the_previous_quoted_list() {
    let expected = "- a\n\n  ```\n  b\n  ```\n```\nc\n```\n> y\n";
    for fence in ["```", "~~~"] {
        let source = format!("- a\n\n  ```\n  b\n  ```\n{fence}\nc\n{fence}\n > y\n");
        let imported = carve::djot_to_carve(&source);
        assert_eq!(
            carve::to_html(&imported),
            carve::to_html(expected),
            "{source:?}\n{imported}"
        );
    }
}

#[test]
fn long_inherited_reference_attributes_stay_shared() {
    let value = "v".repeat(32768);
    for attribute in ["title", "class", "data-long"] {
        let source = format!(
            "{{{attribute}=\"{value}\"}}\n[r]: /u\n\n{}",
            "[x][r] ".repeat(1024)
        );
        let result = carve::djot_to_carve(&source);
        assert!(result.len() < source.len() * 2);
        assert_eq!(result.matches(&value).count(), 1);
    }
}

#[test]
fn class_overrides_keep_other_inherited_attributes_shared() {
    let value = "v".repeat(32768);
    let source = format!(
        "{{.base title=\"{value}\"}}\n[r]: /u\n\n[x][r] {}",
        "[x][r]{.own} ".repeat(1024)
    );
    let result = carve::djot_to_carve(&source);
    assert!(result.len() < source.len() * 3);
    assert!(result.matches(&value).count() <= 2);
    let small = "{.base title=\"a|b\"}\n[r]: /u\n\n| [x][r] | [x][r]{.own} |\n";
    let expected = "| [x](/u){.base title=\"a\\|b\"} | [x](/u){title=\"a\\|b\" .own} |\n";
    assert_eq!(
        carve::to_html(&carve::djot_to_carve(small)),
        carve::to_html(expected)
    );
}

#[test]
fn multiline_code_fallback_reports_plain_export_loss() {
    let result = carve::migrate_djot("a `b\n> c` d\n");
    let losses: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|row| row.code == "structure-unspellable")
        .collect();
    assert_eq!(losses.len(), 1);
    assert_eq!(losses[0].path.as_deref(), Some("line:1"));
    assert_eq!(
        losses[0].message,
        "Multiline inline code uses raw HTML; plain-text export omits it."
    );
}

#[test]
fn container_prefixes_do_not_report_code_loss() {
    for source in [
        "> a `b\n> c` d\n",
        "- > a `b\n  > c` d\n",
        "ref[^n]\n\n[^n]: a\n\np `b\nc` d\n",
        "ref[^n]\n\n[^n]: a\n> p `b\n> c` d\n",
    ] {
        let result = carve::migrate_djot(source);
        assert!(
            !result
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "structure-unspellable"),
            "{source:?}"
        );
    }
}

#[test]
fn output_growth_is_linear_for_wide_numeric_list_markers() {
    for size in [64, 128, 256] {
        let source = format!("{}1. > a\n{}", "0".repeat(size), " > b\n".repeat(size));
        let result = carve::djot_to_carve(&source);
        assert!(result.len() <= source.len() * 2);
        let expected = format!("1. > a\n{}", "   > b\n".repeat(size));
        assert_eq!(carve::to_html(&result), carve::to_html(&expected));
    }
}

#[test]
fn multiline_footnote_code_loss_uses_the_original_opener() {
    let result = carve::migrate_djot("ref[^n]\n\n[^n]: a `b\nc` d\n");
    let losses: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|row| row.code == "structure-unspellable")
        .collect();
    assert_eq!(losses.len(), 1);
    assert_eq!(losses[0].path.as_deref(), Some("line:3"));
    assert_eq!(
        losses[0].message,
        "Multiline inline code uses raw HTML; plain-text export omits it."
    );
}

#[test]
fn nested_notes_with_lazy_continuation_have_bounded_output() {
    for size in [16, 32, 64] {
        let headers = (0..size)
            .map(|n| format!("{}[^n{n}]: a", " ".repeat(n)))
            .collect::<Vec<_>>()
            .join("\n\n");
        let references = (0..size)
            .map(|n| format!("[^n{n}]"))
            .collect::<Vec<_>>()
            .join(" ");
        let source = format!("{headers}\n{}\n{references}\n", "lazy\n".repeat(4096));
        let result = carve::djot_to_carve(&source);
        assert!(result.len() <= source.len() * 2);
        let html = carve::to_html(&result);
        assert_eq!(html.matches("lazy").count(), 4096);
        assert_eq!(html.matches("role=\"doc-noteref\"").count(), size);
    }
}

#[test]
fn deep_inline_lists_with_lazy_quoted_text_have_bounded_output() {
    for size in [16, 32, 64] {
        let source = format!("{}a\n{}", "- > ".repeat(size), " > lazy\n".repeat(1024));
        let result = carve::djot_to_carve(&source);
        assert!(result.len() <= source.len() * 2);
        assert_eq!(carve::to_html(&result).matches("lazy").count(), 1024);
    }
}

#[test]
fn escaped_reference_names_keep_long_destinations_shared() {
    let target = format!("/{}", "x".repeat(32768));
    for shape in ["escaped", "styled", "attributes"] {
        let source = if shape == "escaped" {
            format!("[a\\*b]: {target}\n\n{}", "[link][a\\\\*b] ".repeat(1024))
        } else {
            format!(
                "{}[link]: {target}\n\n{}",
                if shape == "attributes" {
                    "{.target}\n"
                } else {
                    ""
                },
                "[_link_][] ".repeat(1024)
            )
        };
        let result = carve::migrate_djot(&source);
        assert_eq!(result.value.matches(&target).count(), 1, "{shape}");
        assert!(result.value.len() < source.len() * 2, "{shape}");
        assert!(
            !result
                .report
                .diagnostics
                .iter()
                .any(|row| row.message.contains("unresolved Djot reference")),
            "{shape}"
        );
    }
}

#[test]
fn nonbreaking_label_whitespace_does_not_resolve_space_definition() {
    for label in ["a b", "a\u{00a0}b"] {
        let result = carve::migrate_djot(&format!("[{label}]: /x\n\n[a\\ b][]\n"));
        assert!(!carve::to_html(&result.value).contains("href=\"/x\""));
        assert_eq!(
            result
                .report
                .diagnostics
                .iter()
                .filter(|row| row.message.contains("unresolved Djot reference"))
                .count(),
            1
        );
    }
}

#[test]
fn pending_block_attributes_match_djot() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/djot-pending-attributes.json")).unwrap();
    let normalize = |html: &str| {
        let html = html.replace("<tbody>", "").replace("</tbody>", "");
        let html = html.split_whitespace().collect::<Vec<_>>().join(" ");
        html.replace("> ", ">").replace(" <", "<")
    };
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let expected = row["html"].as_str().unwrap();
        let imported = carve::djot_to_carve(source);
        assert_eq!(
            normalize(&carve::to_html(&imported)),
            normalize(expected),
            "{source:?}\n{imported}"
        );
    }
}

#[test]
fn long_pending_values_survive_chained_quote_exits() {
    let value = "v".repeat(32768);
    let classes: Vec<String> = (0..63).map(|index| format!("c{index}")).collect();
    let mut source = format!("{}{{title=\"{value}\"}}\n", "> ".repeat(64));
    for (index, name) in classes.iter().enumerate() {
        source.push_str(&format!("{}{{.{name}}}\n", "> ".repeat(63 - index)));
    }
    source.push_str("\nword");
    let imported = carve::djot_to_carve(&source);
    assert_eq!(imported.matches(&value).count(), 1);
    assert!(imported.len() < source.len() * 2);
    assert!(carve::to_html(&imported).contains(&format!(
        "<p title=\"{value}\" class=\"{}\">word</p>",
        classes.join(" ")
    )));
}

#[test]
fn references_after_footnotes_have_no_false_unresolved_loss() {
    for padding in [" ", "", "\t"] {
        let source = format!("[^n]: note\n  more\n[r]:{padding}/u\n\n[x][r]");
        let result = carve::migrate_djot(&source);
        assert!(carve::to_html(&result.value).contains("href=\"/u\""));
        assert!(!result
            .report
            .diagnostics
            .iter()
            .any(|row| row.message.contains("unresolved Djot reference")));
    }
}

#[test]
fn paragraph_footnote_does_not_open_reference_boundary() {
    let result = carve::migrate_djot("text\n[^n]: note\n[r]: /u\n\n[x][r]");
    assert!(!carve::to_html(&result.value).contains("href=\"/u\""));
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .filter(|row| row.message.contains("unresolved Djot reference"))
            .count(),
        1
    );
}

#[test]
fn adjacent_footnotes_remain_footnotes_beside_caret_reference() {
    let result = carve::migrate_djot("[^n]:/u\n\n[^n][^n]");
    let html = carve::to_html(&result.value);
    assert_eq!(html.matches("role=\"doc-noteref\"").count(), 2);
    assert!(!html.contains("carve-djot-reference"));
}

#[test]
fn quoted_empty_items_keep_nested_blocks() {
    for body in ["> b", "# h", "***", "::: box\n>   text\n>   :::"] {
        let source = format!("> -\n>   {body}\n");
        let expected = format!("> - %%\n>   {body}\n");
        let imported = carve::djot_to_carve(&source);
        assert_eq!(
            carve::to_html(&imported),
            carve::to_html(&expected),
            "{source:?}\n{imported}"
        );
    }
}

#[test]
fn a_closed_list_does_not_own_a_later_image_label() {
    let source = "- a\n\n![x\n- b](u)\n";
    let imported = carve::djot_to_carve(source);
    assert_eq!(
        carve::to_html(&imported),
        carve::to_html("- a\n\n{%%}![x\n\\- b](u)\n"),
        "{imported}"
    );
}

#[test]
fn a_narrow_outer_div_closer_releases_inner_div_ownership() {
    let source = "> ::: a\n> :::: b\n> x\n> :::\n> y\n> :::\n";
    let imported = carve::djot_to_carve(source);
    assert!(imported.ends_with("> \\:::\n"), "{imported}");
    assert!(
        carve::to_html(&imported).contains("<p>y\n:::</p>"),
        "{imported}"
    );
}
