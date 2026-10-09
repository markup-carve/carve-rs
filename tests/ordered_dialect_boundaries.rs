use carve::{html_to_carve, to_carve, to_html, HtmlImportOptions};

#[test]
fn a_dialect_cannot_cross_a_hard_list_boundary() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ordered-dialect-boundaries.json")).unwrap();
    for row in cases.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let html = row["html"].as_str().unwrap();
        assert_eq!(to_html(source).trim(), html, "{}", row["name"]);
        assert_eq!(to_carve(source), source, "{}", row["name"]);
        let import = html_to_carve(
            row["inputHtml"].as_str().unwrap(),
            &HtmlImportOptions::default(),
        )
        .unwrap();
        assert_eq!(import.value, source, "{}", row["name"]);
        assert!(
            import.report.diagnostics.is_empty(),
            "{}: {:?}",
            row["name"],
            import.report.diagnostics
        );
        assert_eq!(to_html(&import.value).trim(), html, "{}", row["name"]);
    }
}

#[test]
fn two_blank_lines_still_permit_the_sibling_tie_break() {
    assert!(to_html("v. x\n\n\nvi. y\n").contains("<ol type=\"i\" start=\"5\">"));
}

#[test]
fn three_blanks_inside_the_item_body_do_not_end_the_sibling_search() {
    assert!(to_html("x. one\n\n\n\n   body\n\nxi. two\n").contains("<ol type=\"i\" start=\"10\">"));
}
