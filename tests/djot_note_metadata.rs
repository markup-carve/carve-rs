use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    source: String,
    loss_lines: Vec<usize>,
    html: Option<String>,
}

fn normalized_html(html: &str) -> String {
    let labels = regex::Regex::new(r#" aria-label="[^"]*""#).unwrap();
    let html = labels.replace_all(html.trim(), "").replace("↩︎", "↩");
    let html = regex::Regex::new(r"(<li>)\n")
        .unwrap()
        .replace_all(&html, "$1")
        .into_owned();
    let html = regex::Regex::new(r"\n(</li>)")
        .unwrap()
        .replace_all(&html, "$1")
        .into_owned();
    let html = html.replace("<tbody>", "").replace("</tbody>", "");
    let html = regex::Regex::new(r#"<ol type="([^"]+)" start="([^"]+)">"#)
        .unwrap()
        .replace_all(&html, "<ol start=\"$2\" type=\"$1\">")
        .into_owned();
    regex::Regex::new(r">\s+<")
        .unwrap()
        .replace_all(&html, "><")
        .into_owned()
}

#[test]
fn definition_attributes_do_not_reach_later_content() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/djot-note-metadata.json")).unwrap();
    for row in cases {
        let result = carve::migrate_djot(&row.source);
        assert_eq!(
            result.value,
            carve::djot_to_carve(&row.source),
            "{}",
            row.name
        );
        let losses: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .filter(|d| d.code == "djot-footnote-definition-attributes-dropped")
            .collect();
        assert_eq!(
            losses.iter().map(|d| d.path.clone()).collect::<Vec<_>>(),
            row.loss_lines
                .iter()
                .map(|line| Some(format!("line:{line}")))
                .collect::<Vec<_>>(),
            "{}",
            row.name
        );
        for loss in losses {
            assert_eq!(
                loss.fidelity,
                carve::MigrationFidelity::Dropped,
                "{}",
                row.name
            );
            assert_eq!(
                loss.confidence,
                carve::MigrationConfidence::Exact,
                "{}",
                row.name
            );
        }
        if let Some(html) = row.html {
            assert_eq!(
                normalized_html(&carve::to_html(&result.value)),
                html,
                "{}",
                row.name
            );
        }
    }
}
