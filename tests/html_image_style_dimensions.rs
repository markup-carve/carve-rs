use carve::{migrate_html, to_html, HtmlImportOptions};

#[test]
fn image_styles_map_integral_pixels_in_either_attribute_order() {
    for html in [
        "<img src=\"x\" alt=\"x\" style=\"width:20px;height:30px\">",
        "<img style=\"height:30px;width:20px\" src=\"x\" alt=\"x\">",
        "<img src=\"x\" alt=\"x\" width=\"10\" height=\"11\" style=\"width:20px;height:30px\">",
        "<img style=\"width:20px;height:30px\" width=\"10\" height=\"11\" src=\"x\" alt=\"x\">",
        "<img src=\"x\" alt=\"x\" style=\"width:020.0px;height:+30px\">",
    ] {
        let result = migrate_html(html, &HtmlImportOptions::default()).unwrap();
        assert!(result.value.contains("width=20"), "{}", result.value);
        assert!(result.value.contains("height=30"), "{}", result.value);
        assert!(to_html(&result.value).contains("width=\"20\""));
    }
    let zero = migrate_html(
        "<img src=\"x\" alt=\"x\" style=\"width:0;height:5px\">",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert!(zero.value.contains("width=0"), "{}", zero.value);
    assert!(zero.value.contains("height=5"), "{}", zero.value);
    let result = migrate_html(
        "<img src=\"x\" alt=\"x\" style=\"width:50%;height:auto\">",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .filter(|row| row.code == "style-unmapped")
            .count(),
        2
    );
}
