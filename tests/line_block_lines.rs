use carve::{BlockNode, CheckedRenderOptions, RenderTarget};

const LINES: &str = r#"{"type":"document","srcByteLength":0,"children":[{"type":"line_block","children":[{"type":"paragraph","children":[{"type":"strong","children":[{"type":"text","value":"Roses are red"},{"type":"hard_break"},{"type":"text","value":"Violets are blue"}]},{"type":"hard_break"}]}],"lines":[["/children/0/children/1","/children/-"]]}]}"#;

#[test]
fn line_ranges_survive_interchange_and_render_from_children() {
    let document = carve::from_json(LINES).unwrap();
    let BlockNode::LineBlock(block) = &document.children[0] else {
        panic!("line block lost");
    };
    assert_eq!(
        block.lines.as_ref().unwrap()[0],
        ["/children/0/children/1", "/children/-"]
    );
    let written = carve::to_json(&document);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&written).unwrap(),
        serde_json::from_str::<serde_json::Value>(LINES).unwrap()
    );
    assert_eq!(carve::from_json(&written).unwrap(), document);
    let rendered =
        carve::with_render_loss_report(RenderTarget::Html, CheckedRenderOptions::default(), || {
            carve::render_html(&document).unwrap()
        })
        .unwrap();
    assert!(rendered.value.contains("Roses are red"));
    assert!(rendered.value.contains("Violets are blue"));
}

#[test]
fn line_ranges_reject_malformed_structure() {
    for bad in [
        LINES.replace("/children/0/children/1", "not-a-pointer"),
        LINES.replace("/children/-\"]]", "/children/0/children/1\"]]"),
        LINES.replace("/children/-\"]]", "/children/-\",\"/children/-\"]]"),
        LINES.replace("/children/-\"]]", "/children/~2\",\"/children/-\"]]"),
        LINES.replace("\"lines\":[[", "\"lines\":[] ,\"unused\":[["),
    ] {
        assert!(carve::from_json(&bad).is_err(), "{bad}");
    }
}
