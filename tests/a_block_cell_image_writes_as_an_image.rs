//! A block image in a flattened table cell contributes itself as an inline
//! image (PART 12 §27), its description on the cell's one line.

fn table_with(blocks: &str) -> String {
    format!(
        r#"{{"type":"document","srcByteLength":0,"children":[{{"type":"table","rows":[{{"type":"table_row","cells":[{{"type":"table_cell","header":true,"children":[{{"type":"text","value":"h"}}]}}]}},{{"type":"table_row","cells":[{{"type":"table_cell","header":false,"blocks":{blocks}}}]}}]}}]}}"#
    )
}

#[test]
fn markdown_writes_the_image() {
    let doc = carve::from_json(&table_with(
        r#"[{"type":"image","src":"x.png","alt":"alt"}]"#,
    ))
    .unwrap();
    assert!(carve::render_markdown(&doc)
        .unwrap()
        .contains("| ![alt](x.png) |"));
}

#[test]
fn plain_and_ansi_write_the_description() {
    let doc = carve::from_json(&table_with(
        r#"[{"type":"image","src":"x.png","alt":"alt"}]"#,
    ))
    .unwrap();
    for out in [
        carve::render_plain_text(&doc).unwrap(),
        carve::render_ansi(&doc).unwrap(),
    ] {
        assert!(out.contains("alt"), "{out}");
        assert!(!out.contains("x.png"), "{out}");
    }
}

#[test]
fn a_figures_image_target_writes_as_an_image_too() {
    let doc = carve::from_json(&table_with(
        r#"[{"type":"figure","target":{"type":"image","src":"x.png","alt":"alt"},"caption":[{"type":"text","value":"cap"}]}]"#,
    ))
    .unwrap();
    assert!(carve::render_markdown(&doc)
        .unwrap()
        .contains("![alt](x.png)"));
}

#[test]
fn a_multiline_description_stays_on_the_row() {
    let doc = carve::from_json(&table_with(
        r#"[{"type":"image","src":"x.png","alt":"a\nb"}]"#,
    ))
    .unwrap();
    let markdown = carve::render_markdown(&doc).unwrap();
    assert!(markdown.contains("| ![a b](x.png) |"), "{markdown}");
    assert_eq!(markdown.lines().count(), 3, "{markdown}");
}
