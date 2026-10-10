use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    source: String,
    html: String,
}

#[test]
fn destination_boundaries() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/djot-destination-boundaries.json")).unwrap();
    let nested_indent = regex::Regex::new(r"\n[ \t]+(<[ou]l>)").unwrap();
    let between_tags = regex::Regex::new(r">\s+<").unwrap();
    let image_attributes = regex::Regex::new(r#"<img alt="([^"]*)" src="([^"]*)">"#).unwrap();
    let urls = regex::Regex::new(r#"(?:href|src)="([^"]*)""#).unwrap();
    for row in cases {
        let html = carve::to_html(&carve::djot_to_carve(&row.source))
            .trim()
            .replace("&nbsp;", "\u{a0}")
            .replace("<tbody>", "")
            .replace("</tbody>", "");
        let html = urls.replace_all(&html, |caps: &regex::Captures<'_>| {
            caps[0].replace(
                &caps[1],
                &caps[1]
                    .replace('(', "%28")
                    .replace(')', "%29")
                    .replace('`', "%60"),
            )
        });
        let html = nested_indent.replace_all(&html, "\n$1");
        assert_eq!(
            between_tags.replace_all(&html, "><"),
            image_attributes.replace_all(
                &row.html.replace("&nbsp;", "\u{a0}"),
                r#"<img src="$2" alt="$1">"#
            ),
            "{}",
            row.name
        );
    }
}

#[test]
fn link_immediately_after_footnote() {
    let html = carve::to_html(&carve::djot_to_carve("note[^1][t](u~x~y)\n\n[^1]: note"));
    assert!(html.contains(r#"<a href="u~x~y">t</a>"#));
    assert!(regex::Regex::new(r">\s+<")
        .unwrap()
        .replace_all(&html, "><")
        .contains("<li id=\"fn1\"><p>note"));
}

#[test]
fn percent_escapes_stay_exact() {
    let source = "[t](u%28x%29y)";
    let converted = carve::djot_to_carve(source);
    assert_eq!(converted, source);
    assert!(carve::to_html(&converted).contains(r#"href="u%28x%29y""#));
}

#[test]
fn footnote_suffix_parentheses_remain_text() {
    for prefix in ["", "!"] {
        let html = carve::to_html(&carve::djot_to_carve(&format!(
            "{prefix}[^n](a ~b~ c)\n\n[^n]: note"
        )));
        assert!(html.contains("(a <sub>b</sub> c)"));
        assert!(html.contains(r##"href="#fn1""##));
        if !prefix.is_empty() {
            assert!(html.contains("<p>!<a id=\"fnref1\""));
        }
    }
}
