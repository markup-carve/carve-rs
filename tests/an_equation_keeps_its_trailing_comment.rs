//! A line of display math with a trailing comment is still an equation, and
//! the comment stays the paragraph's child, so the paragraph's span holds
//! nothing that belongs to no node (CARVE-P12-014, markup-carve/carve-rs#3871).

use serde_json::Value;

fn ast(source: &str) -> Value {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    serde_json::from_str(&carve::to_json(&carve::parse_with_options(
        source, &options,
    )))
    .expect("the serializer emits JSON")
}

fn paragraph(source: &str) -> Value {
    let doc = ast(source);
    let block = &doc["children"][0];
    match block["type"].as_str() {
        Some("figure") => block["target"].clone(),
        _ => block.clone(),
    }
}

#[test]
fn the_comment_stays_beside_the_math() {
    for (source, end) in [
        ("$$`a` %% c\n^ cap\n", 10),
        ("$$`a` %% c\n", 10),
        ("$$`a` %% c\n\n^ cap\n", 10),
        ("$$`a\nb` %% c\n^ cap\n", 12),
    ] {
        let paragraph = paragraph(source);
        let children = paragraph["children"].as_array().unwrap();
        let types: Vec<&str> = children
            .iter()
            .map(|c| c["type"].as_str().unwrap())
            .collect();
        assert_eq!(types, ["math", "comment"], "{source:?}");
        assert_eq!(paragraph["pos"]["endOffset"], end, "{source:?}");
        assert_eq!(children[1]["pos"]["endOffset"], end, "{source:?}");
    }
}

#[test]
fn the_equation_still_takes_its_caption() {
    assert_eq!(
        carve::to_html("$$`a` %% c\n^ cap\n"),
        carve::to_html("$$`a`\n^ cap\n")
    );
    assert_eq!(
        carve::to_carve("$$`a` %% c\n^ cap\n"),
        "$$`a` %% c\n^ cap\n"
    );
}
