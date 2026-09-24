use carve::{
    from_json, render_ansi, render_carve, render_html, render_markdown, render_plain_text,
    BlockNode, Document, Figure, FigureGroup, FigureTarget, InlineNode, Paragraph,
};
use std::collections::BTreeMap;

fn figure(target: &str, caption: &str) -> Figure {
    Figure {
        attrs: None,
        target: Box::new(FigureTarget::Paragraph(Paragraph {
            children: vec![InlineNode::text(target)],
            ..Default::default()
        })),
        rendered_target: None,
        caption: vec![InlineNode::text(caption)],
        short_caption: None,
        pos: None,
    }
}

fn document(children: Vec<BlockNode>) -> Document {
    Document {
        frontmatter: BTreeMap::new(),
        frontmatter_raw: None,
        footnote_defs: BTreeMap::new(),
        footnote_def_pos: BTreeMap::new(),
        children,
        source_len: 0,
        ingest_payload_len: 0,
    }
}

fn render_every_target(doc: &Document) -> Vec<(&'static str, String)> {
    vec![
        ("html", render_html(doc).expect("HTML renders")),
        ("carve", render_carve(doc).expect("Carve renders")),
        ("markdown", render_markdown(doc).expect("Markdown renders")),
        ("plain", render_plain_text(doc).expect("plain text renders")),
        ("ansi", render_ansi(doc).expect("ANSI renders")),
    ]
}

#[test]
fn the_one_target_precedes_the_one_caption_on_every_target() {
    let doc = document(vec![BlockNode::Figure(figure("TGT-ONE", "CAP-ONE"))]);

    for (renderer, output) in render_every_target(&doc) {
        let target = output
            .find("TGT-ONE")
            .unwrap_or_else(|| panic!("{renderer}: {output:?}"));
        let caption = output
            .find("CAP-ONE")
            .unwrap_or_else(|| panic!("{renderer}: {output:?}"));
        assert!(target < caption, "{renderer}: {output:?}");
    }
}

#[test]
fn constructed_figure_group_panels_keep_their_order_and_parts() {
    let group = FigureGroup {
        attrs: None,
        children: vec![
            BlockNode::Figure(figure("target one", "caption one")),
            BlockNode::Figure(figure("target two", "caption two")),
        ],
        caption: Some(vec![InlineNode::text("group caption")]),
        pos: None,
    };
    let doc = document(vec![BlockNode::FigureGroup(group)]);

    for (renderer, output) in render_every_target(&doc) {
        let first = [output.find("target one"), output.find("caption one")];
        let second = [output.find("target two"), output.find("caption two")];
        assert!(first.iter().all(Option::is_some), "{renderer}: {output:?}");
        assert!(second.iter().all(Option::is_some), "{renderer}: {output:?}");
        assert!(
            first.into_iter().flatten().max() < second.into_iter().flatten().min(),
            "{renderer}: {output:?}"
        );
        assert!(output.contains("group caption"), "{renderer}: {output:?}");
    }
}

#[test]
fn plural_figure_fields_are_not_on_the_wire() {
    let valid = r#"{"type":"document","srcByteLength":0,"children":[{"type":"figure","target":{"type":"paragraph","children":[{"type":"text","value":"target"}]},"caption":[{"type":"text","value":"caption"}]}]}"#;
    assert!(from_json(valid).is_ok());

    for field in ["targets", "captions"] {
        let payload = format!(
            r#"{{"type":"document","srcByteLength":0,"children":[{{"type":"figure","target":{{"type":"paragraph","children":[{{"type":"text","value":"target"}}]}},"caption":[{{"type":"text","value":"caption"}}],"{field}":[]}}]}}"#
        );
        let error = from_json(&payload).expect_err("the plural field is outside the schema");
        assert!(error.to_string().contains(field), "{error}");
    }

    for field in ["panels", "captions"] {
        let payload = format!(
            r#"{{"type":"document","srcByteLength":0,"children":[{{"type":"figure_group","children":[],"{field}":[]}}]}}"#
        );
        let error = from_json(&payload).expect_err("the plural field is outside the schema");
        assert!(error.to_string().contains(field), "{error}");
    }
}
