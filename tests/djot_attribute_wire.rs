use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    source: String,
    html: Option<String>,
    cells: Option<usize>,
    #[serde(default)]
    table: bool,
}

#[test]
fn attribute_values_survive_import() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/djot-attribute-wire.json")).unwrap();
    let between_tags = regex::Regex::new(r">\s+<").unwrap();
    for row in cases {
        let mut html = carve::to_html(&carve::djot_to_carve(&row.source))
            .trim()
            .to_owned();
        if let Some(cells) = row.cells {
            assert_eq!(html.matches("<th ").count(), cells, "{}", row.name);
            assert!(!html.contains("<span title="), "{}", row.name);
        } else {
            if row.table {
                html = between_tags.replace_all(&html, "><").into_owned();
                html = html
                    .replace("<thead>", "")
                    .replace("</thead>", "")
                    .replace(" scope=\"col\"", "");
            }
            assert_eq!(html, row.html.unwrap(), "{}", row.name);
        }
    }
}
