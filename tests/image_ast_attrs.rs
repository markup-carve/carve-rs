use serde_json::{json, Value};

fn published(source: &str) -> Value {
    serde_json::from_str(&carve::ast_json::to_json(&carve::parse(source))).unwrap()
}

fn image(tree: &Value) -> &Value {
    &tree["children"][0]["children"][0]
}

#[test]
fn source_block_images_publish_merged_attrs_on_the_image() {
    for source in [
        "{.outer k=one}\n![a](b){.inner k=two}\n",
        "{.outer k=one}\n ![a](b){.inner k=two}\n",
        "{.outer k=one}\n![a][r]{.inner k=two}\n\n[r]: b\n",
    ] {
        let tree = published(source);
        let paragraph = &tree["children"][0];
        assert_eq!(paragraph["blockImage"], true, "{source:?}: {tree}");
        assert!(paragraph.get("attrs").is_none(), "{source:?}: {tree}");
        assert_eq!(image(&tree)["attrs"]["classes"], json!(["outer", "inner"]));
        assert_eq!(image(&tree)["attrs"]["keyValues"]["k"], "two");
    }
}

#[test]
fn ingested_block_images_publish_merged_attrs_on_the_image() {
    let mut tree = published("![a](b){.inner k=two}\n");
    tree["children"][0]["attrs"] = json!({
        "classes": ["outer"],
        "keyValues": {"k": "one"},
        "order": [".class", "k"]
    });
    let ingested = carve::ast_json::from_json(&tree.to_string()).unwrap();
    let republished: Value = serde_json::from_str(&carve::ast_json::to_json(&ingested)).unwrap();
    assert!(republished["children"][0].get("attrs").is_none());
    assert_eq!(
        image(&republished)["attrs"]["classes"],
        json!(["outer", "inner"])
    );
    assert_eq!(image(&republished)["attrs"]["keyValues"]["k"], "two");
}

#[test]
fn source_writer_keeps_a_leading_attribute_line() {
    let source = "{#f}\n![a][r]\n\n[r]: /u";
    assert_eq!(carve::to_carve(source).trim(), source);
}
