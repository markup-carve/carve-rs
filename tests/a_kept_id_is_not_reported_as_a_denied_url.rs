use carve::html_import::{html_to_carve, HtmlImportMode, HtmlImportOptions};

#[test]
fn a_kept_id_is_not_reported_as_a_denied_url() {
    let result = html_to_carve(
        "<form id=\"javascript:x\">x</form>",
        &HtmlImportOptions {
            mode: HtmlImportMode::Roundtrip,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(result.report.diagnostics.iter().all(|diagnostic| {
        !diagnostic.message.contains("id with a denied URL scheme")
    }));
}
