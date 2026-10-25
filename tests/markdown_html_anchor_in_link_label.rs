use carve::{migrate_markdown, to_html};

#[test]
fn flattens_matched_html_anchors_inside_markdown_link_labels() {
    for source in [
        "[outer <a href=\"/inner\">inner</a> tail](/outer)",
        "[outer <a class=\"x\" href=\"/inner\">inner</a> tail](/outer)",
        "[outer <a href=\"/inner\">inner</a> tail][ref]\n\n[ref]: /outer",
        "[outer <a href=\"/inner`path\">inner</a> tail](/outer)",
        "[outer <a href=\"/inner``path\">inner</a> tail](/outer)",
    ] {
        let result = migrate_markdown(source);
        let html = to_html(&result.value);
        assert!(
            html.contains("<a href=\"/outer\">outer inner tail</a>"),
            "{html}"
        );
        assert_eq!(html.matches("<a ").count(), 1, "{html}");
        assert!(result
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "structure-unspellable"));
    }
    let html = to_html(&migrate_markdown("<a href=\"/inner\">inner</a> and [outer](/outer)").value);
    assert!(html.contains("<a href=\"/inner\">inner</a>"), "{html}");
}
