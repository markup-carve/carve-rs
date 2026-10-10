use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    source: String,
    html: String,
    #[serde(rename = "carveHtml")]
    carve_html: Option<String>,
}

#[test]
fn literal_delimiter_boundaries() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/djot-escaped-brace-atoms.json")).unwrap();
    let between_tags = regex::Regex::new(r">\s+<").unwrap();
    let image_attributes = regex::Regex::new(r#"<img alt="([^"]*)" src="([^"]*)">"#).unwrap();
    for row in cases {
        let html = carve::to_html(&carve::djot_to_carve(&row.source))
            .trim()
            .replace("&nbsp;", "\u{a0}")
            .replace("<tbody>", "")
            .replace("</tbody>", "");
        assert_eq!(
            between_tags.replace_all(&html, "><"),
            image_attributes.replace_all(
                &row.carve_html
                    .as_ref()
                    .unwrap_or(&row.html)
                    .replace("&nbsp;", "\u{a0}"),
                r#"<img src="$2" alt="$1">"#
            ),
            "{}",
            row.name
        );
    }
}

#[test]
fn user_placeholders_and_frontmatter_are_preserved() {
    let token = "\0DJOTINVALIDATTR0\0";
    for prefix in [String::new(), format!("---\nlabel: {token}\n---\n\n")] {
        let converted = carve::djot_to_carve(&format!("{prefix}{token} w{{x}}{{.c}}"));
        assert!(converted.contains(token));
        assert!(!converted.contains("\0DJOTINVALIDATTR1\0"));
        if !prefix.is_empty() {
            assert!(converted.starts_with(&prefix));
        }
    }
}

#[test]
fn raw_brace_footnote_label_keeps_its_definition() {
    let converted = carve::djot_to_carve("[^a{b}]: note\n\nsee [^a{b}]");
    let html = carve::to_html(&converted);
    let html = regex::Regex::new(r">\s+<")
        .unwrap()
        .replace_all(&html, "><");
    assert!(html.contains("<li id=\"fn1\"><p>note"));
    assert!(html.contains("href=\"#fn1\""));
    assert!(!html.contains("href=\"#fn2\""));
    assert!(!converted.contains("DJOTINVALIDATTR"));
}
