use carve::{html_to_carve, HtmlImportMode, HtmlImportOptions};

#[test]
fn endnote_item_reports_authored_attributes_at_its_path() {
    let html = "<p><a href=\"#fn1\" role=\"doc-noteref\">1</a></p><section role=\"doc-endnotes\"><ol><li id=\"fn1\" class=\"k\" data-x=\"1\" style=\"color:red\" onclick=\"x()\" role=\"doc-footnote\"><p>body</p></li></ol></section>";
    for mode in ["safe", "semantic", "roundtrip"] {
        let result = html_to_carve(
            html,
            &HtmlImportOptions {
                mode: HtmlImportMode::from_name(mode).unwrap(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(result.value, "[^1]\n\n[^1]: body\n");
        let rows: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .filter(|row| matches!(row.code.as_str(), "attribute-dropped" | "style-unmapped"))
            .collect();
        assert_eq!(rows.len(), 4, "{mode}: {rows:?}");
        for row in &rows {
            assert_eq!(row.path.as_deref(), Some("/section[2]/ol[1]/li[1]"));
        }
        assert!(rows
            .iter()
            .any(|row| row.code.as_str() == "attribute-dropped"
                && row.message.contains("Dropped class")
                && row.severity.as_str() == "info"));
        assert!(rows
            .iter()
            .any(|row| row.code.as_str() == "attribute-dropped"
                && row.message.contains("Dropped data-x")
                && row.severity.as_str() == "info"));
        assert!(rows
            .iter()
            .any(|row| row.code.as_str() == "style-unmapped" && row.message.contains("color")));
        assert!(rows
            .iter()
            .any(|row| row.code.as_str() == "attribute-dropped"
                && row.message.contains("event-handler")
                && row.severity.as_str() == "warning"));
    }
}

#[test]
fn generated_endnote_item_attributes_keep_the_same_diagnostic_contract() {
    let mut checked = 0;
    for mode in ["safe", "semantic", "roundtrip"] {
        for attribute in 0..16 {
            for body in 0..21 {
                let html = format!("<p><a href=\"#fn1\" role=\"doc-noteref\">1</a></p><section role=\"doc-endnotes\"><ol><li id=\"fn1\" data-case-{attribute}=\"v\"><p>body{body}</p></li></ol></section>");
                let result = html_to_carve(
                    &html,
                    &HtmlImportOptions {
                        mode: HtmlImportMode::from_name(mode).unwrap(),
                        ..Default::default()
                    },
                )
                .unwrap();
                let rows: Vec<_> = result
                    .report
                    .diagnostics
                    .iter()
                    .filter(|row| row.code.as_str() == "attribute-dropped")
                    .collect();
                assert_eq!(rows.len(), 1, "{mode}: {html}");
                assert_eq!(rows[0].severity.as_str(), "info");
                assert_eq!(rows[0].path.as_deref(), Some("/section[2]/ol[1]/li[1]"));
                assert!(rows[0].message.contains(&format!("data-case-{attribute}")));
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 1008);
}

#[test]
fn many_endnote_items_keep_their_own_report_paths() {
    let mut refs = String::new();
    let mut items = String::new();
    for n in 1..=64 {
        refs.push_str(&format!("<a href=\"#fn{n}\" role=\"doc-noteref\">{n}</a>"));
        items.push_str(&format!(
            "<li id=\"fn{n}\" data-note=\"{n}\"><p>body{n}</p></li>"
        ));
    }
    let html = format!("<p>{refs}</p><section role=\"doc-endnotes\"><ol>{items}</ol></section>");
    let result = html_to_carve(&html, &HtmlImportOptions::default()).unwrap();
    let drops: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|row| row.code.as_str() == "attribute-dropped")
        .collect();
    assert_eq!(drops.len(), 64);
    for (n, row) in drops.iter().enumerate() {
        assert_eq!(
            row.path.as_deref(),
            Some(format!("/section[2]/ol[1]/li[{}]", n + 1).as_str())
        );
    }
}
