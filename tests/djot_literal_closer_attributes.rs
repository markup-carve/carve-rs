use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    source: String,
    html: String,
}

#[test]
fn literal_delimiter_boundaries() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/djot-literal-closer-attributes.json")).unwrap();
    let between_tags = regex::Regex::new(r">\s+<").unwrap();
    for row in cases {
        let html = carve::to_html(&carve::djot_to_carve(&row.source))
            .trim()
            .replace("<tbody>", "")
            .replace("</tbody>", "");
        assert_eq!(
            between_tags.replace_all(&html, "><"),
            row.html,
            "{}",
            row.name
        );
    }
}
