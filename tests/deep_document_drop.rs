use carve::{
    BlockExtension, BlockNode, BlockQuote, Citation, CitationGroup, DefinitionDef, DefinitionItem,
    DefinitionList, Emphasis, EmphasisKind, Figure, FigureTarget, InlineNode, Paragraph, Ruby,
    RubyPair, Table, TableCell, TableRow, ThematicBreak,
};

const DEPTH: usize = 20_000;

#[test]
fn dropping_deep_block_and_inline_trees_uses_bounded_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut block = BlockNode::ThematicBreak(ThematicBreak::default());
            for _ in 0..DEPTH {
                block = BlockNode::BlockQuote(BlockQuote {
                    attrs: None,
                    children: vec![block],
                    fenced: false,
                    pos: None,
                });
            }

            let mut inline = InlineNode::text("end");
            for _ in 0..DEPTH {
                inline = InlineNode::Emphasis(Emphasis {
                    attrs: None,
                    kind: EmphasisKind::Italic,
                    children: vec![inline],
                    pos: None,
                });
            }

            let mut document = carve::parse("");
            document.children.push(block);
            document.children.push(BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: vec![inline],
                at_content_column: true,
                block_image: false,
                pos: None,
            }));
            drop(document);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn dropping_deep_extension_fallbacks_uses_bounded_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut block = BlockNode::ThematicBreak(ThematicBreak::default());
            for _ in 0..DEPTH {
                block = BlockNode::BlockExtension(BlockExtension {
                    name: "example.node".into(),
                    version: None,
                    fallback: Box::new(block),
                    payload: None,
                    attrs: None,
                    pos: None,
                });
            }
            let mut document = carve::parse("");
            document.footnote_defs.insert("deep".into(), vec![block]);
            drop(document);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn dropping_deep_content_inside_figures_citations_and_definitions_uses_bounded_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut inline = InlineNode::text("end");
            for _ in 0..DEPTH {
                inline = InlineNode::Emphasis(Emphasis {
                    attrs: None,
                    kind: EmphasisKind::Italic,
                    children: vec![inline],
                    pos: None,
                });
            }
            let citation = InlineNode::CitationGroup(CitationGroup {
                items: vec![Citation {
                    key: "key".into(),
                    mode: None,
                    prefix: Some(vec![inline]),
                    locator: None,
                    locator_label: None,
                    locator_value: None,
                    suffix: None,
                    suppress_author: false,
                    number: None,
                    label: None,
                    use_index: None,
                    pos: None,
                }],
                raw: String::new(),
                render_mode: None,
                pos: None,
            });
            let ruby = InlineNode::Ruby(Ruby {
                attrs: None,
                pairs: vec![RubyPair {
                    base: vec![citation],
                    annotation: vec![],
                }],
                pos: None,
            });
            let table = Table {
                attrs: None,
                caption: None,
                short_caption: None,
                columns: vec![],
                rows: vec![TableRow {
                    cells: vec![TableCell {
                        header: false,
                        span: None,
                        colspan: None,
                        rowspan: None,
                        align: None,
                        valign: None,
                        attrs: None,
                        children: vec![ruby],
                        pos: None,
                    }],
                    attrs: None,
                    pos: None,
                }],
                row_groups: None,
                pos: None,
            };
            let figure = BlockNode::Figure(Figure {
                attrs: None,
                target: Box::new(FigureTarget::Table(table)),
                rendered_target: None,
                caption: vec![],
                short_caption: None,
                pos: None,
            });

            let mut block = BlockNode::ThematicBreak(ThematicBreak::default());
            for _ in 0..DEPTH {
                block = BlockNode::BlockQuote(BlockQuote {
                    attrs: None,
                    children: vec![block],
                    fenced: false,
                    pos: None,
                });
            }
            let definitions = BlockNode::DefinitionList(DefinitionList {
                attrs: None,
                items: vec![DefinitionItem {
                    terms: vec![],
                    definitions: vec![DefinitionDef {
                        attrs: None,
                        children: vec![block],
                        pos: None,
                    }],
                    pos: None,
                }],
                loose: false,
                pos: None,
            });

            let mut document = carve::parse("");
            document.children.extend([figure, definitions]);
            drop(document);
        })
        .unwrap()
        .join()
        .unwrap();
}
