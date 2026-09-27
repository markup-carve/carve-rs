use carve::{
    html_to_carve, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions, HtmlImportSeverity,
};

#[test]
fn semantic_marker_collisions_are_reported() {
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        for tag in ["abbr", "kbd", "time", "samp", "var", "cite", "dfn"] {
            for value in ["", "value", "javascript:x()"] {
                let html = format!(r#"<{tag} {tag}="{value}">text</{tag}>"#);
                let result = html_to_carve(
                    &html,
                    &HtmlImportOptions {
                        mode,
                        ..Default::default()
                    },
                )
                .unwrap();
                let rows: Vec<_> = result
                    .report
                    .diagnostics
                    .iter()
                    .filter(|row| row.code == HtmlImportDiagnosticCode::AttributeDropped)
                    .collect();
                assert_eq!(rows.len(), 1, "{html}");
                assert_eq!(rows[0].severity, HtmlImportSeverity::Warning, "{html}");
                assert_eq!(rows[0].path.as_deref(), Some(format!("/{tag}[1]").as_str()));
            }
        }
    }
}

#[test]
fn css_owned_attribute_has_a_valid_preserved_subject() {
    for attrs in [
        r#"align="right" style="text-align:left""#,
        r#"style="text-align:left" align="right""#,
    ] {
        let html = format!("<form><table><tr><td {attrs}>text</td></tr></table></form>");
        let result = html_to_carve(
            &html,
            &HtmlImportOptions {
                mode: HtmlImportMode::Roundtrip,
                ..Default::default()
            },
        )
        .unwrap();
        let rows: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .filter(|row| row.message.starts_with("Preserved align on <td>"))
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].code, HtmlImportDiagnosticCode::AttributePreserved);
        assert_eq!(rows[0].severity, HtmlImportSeverity::Info);
    }
}
