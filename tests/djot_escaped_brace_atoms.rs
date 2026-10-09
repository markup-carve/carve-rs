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
        serde_json::from_str(include_str!("fixtures/djot-escaped-brace-atoms.json")).unwrap();
    let between_tags = regex::Regex::new(r">\s+<").unwrap();
    for row in cases {
        let html = carve::to_html(&carve::djot_to_carve(&row.source))
            .trim()
            .replace("&nbsp;", "\u{a0}")
            .replace("<tbody>", "")
            .replace("</tbody>", "");
        assert_eq!(
            between_tags.replace_all(&html, "><"),
            row.html.replace("&nbsp;", "\u{a0}"),
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
