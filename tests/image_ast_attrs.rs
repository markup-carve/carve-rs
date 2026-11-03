use serde_json::{json, Value};

fn published(source: &str) -> Value {
    serde_json::from_str(&carve::ast_json::to_json(&carve::parse(source))).unwrap()
}

fn image(tree: &Value) -> &Value {
    &tree["children"][0]["children"][0]
}

// Ruled A on markup-carve/carve#5039 (CARVE-P12-043): promotion moves no attributes.
#[test]
fn source_block_images_keep_the_attribute_line_on_the_paragraph() {
    for source in [
        "{.outer k=one}\n![a](b){.inner k=two}\n",
        "{.outer k=one}\n ![a](b){.inner k=two}\n",
        "{.outer k=one}\n![a][r]{.inner k=two}\n\n[r]: b\n",
    ] {
        let tree = published(source);
        let paragraph = &tree["children"][0];
        assert_eq!(paragraph["blockImage"], true, "{source:?}: {tree}");
        assert_eq!(
            paragraph["attrs"]["classes"],
            json!(["outer"]),
            "{source:?}: {tree}"
        );
        assert_eq!(paragraph["attrs"]["keyValues"]["k"], "one");
        assert_eq!(image(&tree)["attrs"]["classes"], json!(["inner"]));
        assert_eq!(image(&tree)["attrs"]["keyValues"]["k"], "two");
        assert_eq!(
            carve::to_html(source),
            carve::render_html(&carve::from_json(&tree.to_string()).unwrap()).unwrap(),
            "{source:?}"
        );
    }
}

#[test]
fn a_reference_image_keeps_the_definition_attributes_apart() {
    let source = "{.c}\n![a][ex]\n\n[ex]: /i.png {.wide}\n";
    let tree = published(source);
    assert_eq!(tree["children"][0]["attrs"]["classes"], json!(["c"]));
    assert_eq!(image(&tree)["attrs"]["classes"], json!(["wide"]));
    let ingested = carve::ast_json::from_json(&tree.to_string()).unwrap();
    assert_eq!(
        carve::render_carve(&ingested).unwrap().trim(),
        source.trim()
    );
    assert_eq!(
        carve::to_html(source).trim(),
        "<img src=\"/i.png\" alt=\"a\" class=\"c wide\">"
    );
}

#[test]
fn ingested_block_images_keep_paragraph_and_image_attrs_apart() {
    let mut tree = published("![a](b){.inner k=two}\n");
    tree["children"][0]["attrs"] = json!({
        "classes": ["outer"],
        "keyValues": {"k": "one"},
        "order": [".class", "k"]
    });
    let ingested = carve::ast_json::from_json(&tree.to_string()).unwrap();
    let republished: Value = serde_json::from_str(&carve::ast_json::to_json(&ingested)).unwrap();
    assert_eq!(
        republished["children"][0]["attrs"]["classes"],
        json!(["outer"])
    );
    assert_eq!(image(&republished)["attrs"]["classes"], json!(["inner"]));
    assert_eq!(image(&republished)["attrs"]["keyValues"]["k"], "two");
}

#[test]
fn source_writer_keeps_a_leading_attribute_line() {
    let source = "{#f}\n![a][r]\n\n[r]: /u";
    assert_eq!(carve::to_carve(source).trim(), source);
}
