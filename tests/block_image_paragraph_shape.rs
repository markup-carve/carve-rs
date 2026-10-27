use carve::{parse, to_html, to_json};

fn first_block(source: &str) -> serde_json::Value {
    let value: serde_json::Value = serde_json::from_str(&to_json(&parse(source))).unwrap();
    value["children"][0].clone()
}

#[test]
fn resolved_image_in_task_item_keeps_its_paragraph() {
    let block = first_block("- [ ] ![alt][ref]\n\n[ref]: x.png\n");
    let image = &block["items"][0]["children"][0];
    assert_eq!(image["type"], "paragraph");
    assert_eq!(image["blockImage"], true);
    assert_eq!(image["children"][0]["type"], "image");
}

#[test]
fn comment_prefixed_image_keeps_comment_and_paragraph() {
    let block = first_block("{%%}![alt](x.png)\n");
    assert_eq!(block["type"], "paragraph");
    assert_eq!(block["blockImage"], true);
    assert_eq!(block["children"][0]["type"], "comment");
    assert_eq!(block["children"][1]["type"], "image");
    assert_eq!(
        to_html("{%%}![alt](x.png)\n").trim(),
        "<img src=\"x.png\" alt=\"alt\">"
    );
}
