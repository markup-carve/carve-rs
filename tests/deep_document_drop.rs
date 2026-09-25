use carve::{
    dispose_blocks, dispose_inlines, BlockExtension, BlockNode, BlockQuote, Citation,
    CitationGroup, DefinitionDef, DefinitionItem, DefinitionList, Emphasis, EmphasisKind, Figure,
    FigureTarget, InlineNode, Paragraph, Ruby, RubyPair, Table, TableCell, TableRow, ThematicBreak,
};

const DEPTH: usize = 20_000;

#[test]
fn disposing_detached_block_and_inline_trees_uses_bounded_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut block = BlockNode::ThematicBreak(ThematicBreak::default());
            let mut inline = InlineNode::text("end");
            for _ in 0..DEPTH {
                block = BlockNode::BlockQuote(BlockQuote {
                    attrs: None,
                    children: vec![block],
                    fenced: false,
                    pos: None,
                });
                inline = InlineNode::Emphasis(Emphasis {
                    attrs: None,
                    kind: EmphasisKind::Italic,
                    children: vec![inline],
                    pos: None,
                });
            }

            dispose_blocks(vec![block]);
            dispose_inlines(vec![inline]);

            let mut document = carve::parse("");
            let mut block = BlockNode::ThematicBreak(ThematicBreak::default());
            for _ in 0..DEPTH {
                block = BlockNode::BlockQuote(BlockQuote {
                    attrs: None,
                    children: vec![block],
                    fenced: false,
                    pos: None,
                });
            }
            document.children.push(block);
            dispose_blocks(std::mem::take(&mut document.children));
        })
        .unwrap()
        .join()
        .unwrap();
}

// The derived traits walk nested fields. This is the measured small-stack
// envelope for these simple caller-built chains, not a universal depth cap.
#[test]
fn recursive_ast_traits_handle_16_levels_on_a_small_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut block = BlockNode::ThematicBreak(ThematicBreak::default());
            let mut inline = InlineNode::text("end");
            for _ in 0..16 {
                block = BlockNode::BlockQuote(BlockQuote {
                    attrs: None,
                    children: vec![block],
                    fenced: false,
                    pos: None,
                });
                inline = InlineNode::Emphasis(Emphasis {
                    attrs: None,
                    kind: EmphasisKind::Italic,
                    children: vec![inline],
                    pos: None,
                });
            }

            let block_clone = block.clone();
            let inline_clone = inline.clone();
            assert_eq!(block, block_clone);
            assert_eq!(inline, inline_clone);
            assert!(!format!("{block:?}").is_empty());
            assert!(!format!("{inline:?}").is_empty());
            dispose_blocks(vec![block, block_clone]);
            dispose_inlines(vec![inline, inline_clone]);

            let mut document = carve::parse("");
            let mut block = BlockNode::ThematicBreak(ThematicBreak::default());
            let mut inline = InlineNode::text("end");
            for _ in 0..16 {
                block = BlockNode::BlockQuote(BlockQuote {
                    attrs: None,
                    children: vec![block],
                    fenced: false,
                    pos: None,
                });
                inline = InlineNode::Emphasis(Emphasis {
                    attrs: None,
                    kind: EmphasisKind::Italic,
                    children: vec![inline],
                    pos: None,
                });
            }
            document.children.push(block);
            document.children.push(BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: vec![inline],
                at_content_column: true,
                block_image: false,
                pos: None,
            }));
            let cloned = document.clone();
            assert_eq!(document, cloned);
            assert!(!format!("{document:?}").is_empty());
        })
        .unwrap()
        .join()
        .unwrap();
}

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
