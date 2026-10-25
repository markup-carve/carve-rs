use carve::{html_to_carve, to_html, HtmlImportMode, HtmlImportOptions};

#[test]
fn an_empty_left_aligned_header_keeps_its_column_default() {
    let html = "<table><tr><th style=\"text-align:left\"></th></tr>\
<tr><td style=\"text-align:left\">a</td></tr></table>";
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();

    assert_eq!(result.value, "{aligns=left}\n|= |\n| a |\n");
    assert!(result.report.diagnostics.is_empty());
    assert!(to_html(&result.value).contains("<th scope=\"col\" style=\"text-align: left;\"></th>"));
    assert!(to_html(&result.value).contains("<td style=\"text-align: left;\">a</td>"));
}

#[test]
fn an_empty_top_aligned_header_keeps_its_vertical_default() {
    let html = "<table><tr><th style=\"vertical-align:top\"></th></tr>\
<tr><td style=\"vertical-align:top\">a</td></tr></table>";
    let result = html_to_carve(
        html,
        &HtmlImportOptions {
            mode: HtmlImportMode::Semantic,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(
        result.value.starts_with("{valigns=top}\n"),
        "{}",
        result.value
    );
    assert!(
        to_html(&result.value).contains("<th scope=\"col\" style=\"vertical-align: top;\"></th>")
    );
}

#[test]
fn an_empty_header_between_other_columns_uses_its_physical_index() {
    let html = "<table><tr><th>A</th><th style=\"text-align:left\"></th><th>Z</th></tr>\
<tr><td>a</td><td>b</td><td>z</td></tr></table>";
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();

    assert!(result.value.starts_with("{aligns=,left}\n"));
    assert!(to_html(&result.value).contains("<th scope=\"col\" style=\"text-align: left;\"></th>"));
}
