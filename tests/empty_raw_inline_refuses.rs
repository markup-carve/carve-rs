use carve::{
    render_carve, BlockNode, Document, InlineNode, Paragraph, RawInline, RenderCarveError,
};
use std::collections::BTreeMap;

#[test]
fn an_empty_raw_inline_refuses_instead_of_emitting_a_different_tree() {
    let doc = Document {
        frontmatter: BTreeMap::new(),
        frontmatter_raw: None,
        footnote_defs: BTreeMap::new(),
        footnote_def_pos: BTreeMap::new(),
        children: vec![BlockNode::Paragraph(Paragraph {
            children: vec![InlineNode::RawInline(RawInline {
                format: "html".into(),
                content: String::new(),
                injected: false,
                pos: None,
            })],
            ..Default::default()
        })],
        source_len: 0,
        ingest_payload_len: 0,
    };

    let error = render_carve(&doc).expect_err("the node has no source spelling");
    match error {
        RenderCarveError::SourceUnspellable(error) => {
            assert_eq!(error.node_type(), "raw_inline");
            assert_eq!(
                error.reason(),
                "an empty raw inline has no Carve source spelling"
            );
        }
        other => panic!("unexpected refusal: {other}"),
    }
}
