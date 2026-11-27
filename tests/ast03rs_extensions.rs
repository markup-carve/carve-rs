use serde_json::{Value, json};

fn parse(source: &str, extension: &str) -> Value {
    let request = json!({"call": "parse", "source": source,
        "options": {"extensions": [extension]}});
    let response: Value =
        serde_json::from_str(&carve::api::json::call(&request.to_string())).unwrap();
    assert!(response.get("ok").is_some(), "{response}");
    response["ok"]["tree"].clone()
}

#[test]
fn wiki_and_cite_keep_their_parsed_props() {
    let wiki = parse(":wiki[Page#anchor|Label]", "carve.wiki");
    let node = &wiki["children"][0]["children"][0];
    assert_eq!(node["props"], json!({"target": "Page#anchor"}));
    assert_eq!(node["fallback"], json!([{"type":"text","value":"Label"}]));

    let cite = parse(":cite[doe; smith]", "carve.cite");
    let node = &cite["children"][0]["children"][0];
    assert_eq!(node["props"], json!({"keys": ["doe", "smith"]}));
}

#[test]
fn wiki_and_cite_props_read_back_without_a_host() {
    for (source, extension) in [
        (":wiki[Page]\n", "carve.wiki"),
        (":cite[doe; smith]\n", "carve.cite"),
    ] {
        let tree = parse(source, extension);
        let request = json!({"call":"writeWithDiagnostics", "tree":tree});
        let response: Value =
            serde_json::from_str(&carve::api::json::call(&request.to_string())).unwrap();
        assert_eq!(response["ok"]["output"], source, "{response}");
        assert_eq!(response["ok"]["diagnostics"], json!([]));
    }
}

#[test]
fn a_citation_without_a_key_keeps_its_core_form() {
    let tree = parse(":cite[; ; ]", "carve.cite");
    let node = &tree["children"][0]["children"][0];
    assert_eq!(node["name"], "cite");
    assert!(node.get("props").is_none());
}

#[test]
fn image_fence_keeps_payload_and_positioned_fallback() {
    let tree = parse("```img\n<svg/>\n```", "carve.imgFence.img");
    let node = &tree["children"][0];
    assert_eq!(node["name"], "carve.imgFence.img");
    assert_eq!(
        node["payload"],
        json!([{"format":"image/svg+xml","value":"<svg/>\n"}])
    );
    assert_eq!(node["fallback"][0]["pos"], node["pos"]);
}

#[test]
fn hostile_image_source_is_still_claimed_and_falls_back_on_render() {
    let source = "```img\n<script>alert(1)</script>\n```";
    let tree = parse(source, "carve.imgFence.img");
    assert_eq!(tree["children"][0]["name"], "carve.imgFence.img");
    let request = json!({"call":"renderWithDiagnostics", "source":source,
        "options":{"extensions":["carve.imgFence.img"],
            "renderers":{"carve.imgFence.img":"standard"}}});
    let response: Value =
        serde_json::from_str(&carve::api::json::call(&request.to_string())).unwrap();
    assert!(
        response["ok"]["html"]
            .as_str()
            .unwrap()
            .starts_with("<pre><code")
    );
    assert!(
        response["ok"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["code"] == "extension-renderer-failed")
    );
}

#[test]
fn image_fence_claim_does_not_prejudge_svg_safety() {
    let tree = parse("```img\nplain text\n```", "carve.imgFence.img");
    let node = &tree["children"][0];
    assert_eq!(node["name"], "carve.imgFence.img");
    assert_eq!(
        node["payload"],
        json!([{"format":"image/svg+xml","value":"plain text\n"}])
    );
}

#[test]
fn disclosures_use_the_declared_title_slot() {
    for (name, extension) in [
        ("details", "carve.details"),
        ("spoiler", "carve.spoilerBlock"),
    ] {
        let source = format!("::: {name} \"More\"\nHidden.\n:::");
        let tree = parse(&source, extension);
        let node = &tree["children"][0];
        assert_eq!(node["slots"]["title"][0]["value"], "More");
        assert!(node["slots"].get("summary").is_none());
        assert_eq!(tree["extensions"][0]["slots"], json!(["title", "body"]));
    }
}

#[test]
fn extension_field_variants_keep_the_same_contract() {
    // 250 inputs per claim family exercise source-derived fields with changing
    // content, sibling text, both image-fence names, and both disclosure names.
    for i in 0..250 {
        let wiki = parse(
            &format!("before :wiki[Page{i}|Label{i}] after"),
            "carve.wiki",
        );
        assert_eq!(
            wiki["children"][0]["children"][1]["props"],
            json!({"target": format!("Page{i}")})
        );

        let cite = parse(
            &format!("before :cite[key{i}; other{i}] after"),
            "carve.cite",
        );
        assert_eq!(
            cite["children"][0]["children"][1]["props"],
            json!({"keys": [format!("key{i}"), format!("other{i}")]})
        );

        let (lang, ext) = if i % 2 == 0 {
            ("img", "carve.imgFence.img")
        } else {
            ("image", "carve.imgFence.image")
        };
        let body = if i % 3 == 0 {
            format!("<script>{i}</script>\n")
        } else {
            format!("<svg><text>{i}</text></svg>\n")
        };
        let tree = parse(&format!("```{lang}\n{body}```"), ext);
        let node = &tree["children"][0];
        assert_eq!(node["name"], ext);
        assert_eq!(
            node["payload"],
            json!([{"format":"image/svg+xml","value":body}])
        );
        assert_eq!(node["fallback"][0]["pos"], node["pos"]);

        let (name, ext) = if i % 2 == 0 {
            ("details", "carve.details")
        } else {
            ("spoiler", "carve.spoilerBlock")
        };
        let tree = parse(&format!("::: {name} \"Title{i}\"\nBody{i}.\n:::"), ext);
        let node = &tree["children"][0];
        assert_eq!(node["slots"]["title"][0]["value"], format!("Title{i}"));
        assert!(node["slots"].get("summary").is_none());
    }
}
