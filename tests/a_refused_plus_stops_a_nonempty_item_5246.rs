use serde_json::Value;

fn ast(source: &str) -> Value {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    serde_json::from_str(&carve::to_json(&carve::parse_with_options(
        source, &options,
    )))
    .unwrap()
}

#[test]
fn a_shifted_refused_plus_does_not_extend_the_item_or_list() {
    let source = ">\n+\n - e\n +\n";
    let doc = ast(source);
    let list = &doc["children"][1];
    let item = &list["items"][0];
    assert_eq!(item["children"][0]["pos"]["endOffset"], 8);
    assert_eq!(item["pos"]["endOffset"], 8);
    assert_eq!(list["pos"]["endOffset"], 8);
}

#[test]
fn an_empty_item_keeps_its_consumed_plus_extent() {
    let doc = ast("- +\n- b\n");
    let item = &doc["children"][0]["items"][0];
    assert_eq!(item["children"].as_array().unwrap().len(), 0);
    assert_eq!(item["pos"]["endOffset"], 3);
}
