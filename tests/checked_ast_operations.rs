use crate::common;
use carve::*;

fn quote_chain(levels: usize) -> BlockNode {
    let mut node = BlockNode::ThematicBreak(ThematicBreak::default());
    for _ in 0..levels {
        node = BlockNode::BlockQuote(BlockQuote {
            attrs: None,
            children: vec![node],
            fenced: false,
            pos: None,
        });
    }
    node
}
fn inline_chain(levels: usize) -> Vec<InlineNode> {
    let mut nodes = vec![InlineNode::Text(Text {
        value: "leaf".into(),
        pos: None,
    })];
    for _ in 0..levels {
        nodes = vec![InlineNode::Emphasis(Emphasis {
            attrs: None,
            kind: EmphasisKind::Strong,
            children: nodes,
            pos: None,
        })];
    }
    nodes
}
fn document(node: BlockNode) -> Document {
    let mut doc = parse("");
    doc.children.push(node);
    doc
}
fn paragraph(children: Vec<InlineNode>) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        at_content_column: false,
        block_image: false,
        attrs: None,
        children,
        pos: None,
    })
}
fn table() -> Table {
    Table {
        attrs: None,
        caption: None,
        short_caption: None,
        columns: vec![],
        rows: vec![],
        row_groups: None,
        pos: None,
    }
}
fn cell() -> TableCell {
    TableCell {
        header: false,
        span: None,
        colspan: None,
        rowspan: None,
        align: None,
        valign: None,
        attrs: None,
        children: vec![],
        blocks: None,
        pos: None,
    }
}
fn figure(target: FigureTarget) -> Figure {
    Figure {
        attrs: None,
        target: Box::new(target),
        rendered_target: None,
        caption: vec![],
        short_caption: None,
        pos: None,
    }
}
fn assert_refused(doc: Document) {
    let shallow = parse("different root metadata");
    assert!(matches!(doc.try_clone(), Err(error) if error.limit() == MAX_CHECKED_AST_DEPTH));
    assert!(doc.try_eq(&shallow).is_err());
    assert!(shallow.try_eq(&doc).is_err());
    assert!(doc.try_eq(&doc).is_err());
}

#[test]
fn checked_operations_preserve_every_root_field() {
    let mut original =
        parse("---\ntitle: sample\n---\n\n# Heading\n\nText /emphasis/.\n\n[^note]: body\n");
    original.ingest_payload_len = 321;
    let copied = original.try_clone().unwrap();
    assert_eq!(original, copied);
    assert_eq!(original.try_eq(&copied), Ok(true));
    for field in 0..7 {
        let mut changed = copied.try_clone().unwrap();
        match field {
            0 => {
                changed.frontmatter.insert("new".into(), "value".into());
            }
            1 => changed.frontmatter_raw = None,
            2 => {
                changed.footnote_defs.insert("other".into(), vec![]);
            }
            3 => {
                changed
                    .footnote_def_pos
                    .insert("other".into(), Pos::default());
            }
            4 => changed.children.clear(),
            5 => changed.source_len += 1,
            6 => changed.ingest_payload_len += 1,
            _ => unreachable!(),
        }
        assert_eq!(original.try_eq(&changed), Ok(false), "root field {field}");
    }
}

#[test]
fn combined_block_and_inline_nesting_uses_one_budget() {
    let mut block = paragraph(inline_chain(10));
    for _ in 0..10 {
        block = BlockNode::BlockQuote(BlockQuote {
            attrs: None,
            children: vec![block],
            fenced: false,
            pos: None,
        });
    }
    assert_refused(document(block));
}

#[test]
fn hidden_publishing_and_extension_slots_are_checked() {
    let deep = || inline_chain(MAX_CHECKED_AST_DEPTH);
    let mut short = table();
    short.short_caption = Some(deep());
    assert_refused(document(BlockNode::Table(short)));
    for short in [false, true] {
        let mut target = table();
        if short {
            target.short_caption = Some(deep());
        } else {
            target.caption = Some(deep());
        }
        assert_refused(document(BlockNode::Figure(figure(FigureTarget::Table(
            target,
        )))));
    }
    let mut host = figure(FigureTarget::Paragraph(Paragraph {
        at_content_column: false,
        block_image: false,
        attrs: None,
        children: vec![],
        pos: None,
    }));
    host.short_caption = Some(deep());
    assert_refused(document(BlockNode::Figure(host)));
    let mut nested = table();
    let mut content = cell();
    content.blocks = Some(vec![quote_chain(MAX_CHECKED_AST_DEPTH)]);
    nested.rows.push(TableRow {
        cells: vec![content],
        attrs: None,
        pos: None,
    });
    assert_refused(document(BlockNode::Table(nested)));
    assert_refused(document(BlockNode::CitationDefinition(
        CitationDefinition {
            key: "key".into(),
            children: deep(),
            attrs: None,
            pos: None,
        },
    )));
    assert_refused(document(BlockNode::ExtensionCarrier(ExtensionCarrier {
        attrs: None,
        name: "details".into(),
        children: vec![],
        summary: Some(deep()),
        label: None,
        pos: None,
    })));
    assert_refused(document(BlockNode::BlockExtension(BlockExtension {
        name: "example.block".into(),
        version: None,
        fallback: Box::new(quote_chain(MAX_CHECKED_AST_DEPTH)),
        payload: None,
        attrs: None,
        pos: None,
    })));
    let mut notes = parse("");
    notes
        .footnote_defs
        .insert("note".into(), vec![quote_chain(MAX_CHECKED_AST_DEPTH)]);
    assert_refused(notes);
}

#[test]
fn ruby_and_citation_item_slots_are_checked() {
    for annotation in [false, true] {
        let mut pair = RubyPair {
            base: vec![],
            annotation: vec![],
        };
        if annotation {
            pair.annotation = inline_chain(MAX_CHECKED_AST_DEPTH);
        } else {
            pair.base = inline_chain(MAX_CHECKED_AST_DEPTH);
        }
        assert_refused(document(paragraph(vec![InlineNode::Ruby(Ruby {
            attrs: None,
            pairs: vec![pair],
            pos: None,
        })])));
    }
    for slot in 0..3 {
        let mut citation = Citation {
            key: "key".into(),
            mode: None,
            prefix: None,
            locator: None,
            locator_label: None,
            locator_value: None,
            suffix: None,
            suppress_author: false,
            number: None,
            label: None,
            use_index: None,
            pos: None,
        };
        let content = Some(inline_chain(MAX_CHECKED_AST_DEPTH));
        match slot {
            0 => citation.prefix = content,
            1 => citation.locator = content,
            _ => citation.suffix = content,
        }
        assert_refused(document(paragraph(vec![InlineNode::CitationGroup(
            CitationGroup {
                items: vec![citation],
                raw: String::new(),
                render_mode: None,
                pos: None,
            },
        )])));
    }
}

#[test]
fn list_storage_consumes_the_depth_budget() {
    let list = |levels| {
        document(BlockNode::List(List {
            attrs: None,
            ordered: false,
            start: None,
            ol_type: None,
            bare_marker: false,
            delim: None,
            bullet_char: None,
            tight: true,
            items: vec![ListItem {
                attrs: None,
                checked: None,
                task_state: None,
                children: vec![quote_chain(levels)],
                pos: None,
            }],
            pos: None,
        }))
    };
    let boundary = list(MAX_CHECKED_AST_DEPTH - 3);
    let copied = boundary.try_clone().unwrap();
    assert_eq!(boundary.try_eq(&copied), Ok(true));
    assert_refused(list(MAX_CHECKED_AST_DEPTH - 2));

    for term in [false, true] {
        let definition = |levels| {
            document(BlockNode::DefinitionList(DefinitionList {
                attrs: None,
                loose: false,
                pos: None,
                items: vec![DefinitionItem {
                    terms: if term {
                        vec![DefinitionTerm {
                            attrs: None,
                            children: inline_chain(levels),
                            pos: None,
                        }]
                    } else {
                        vec![]
                    },
                    definitions: if term {
                        vec![]
                    } else {
                        vec![DefinitionDef {
                            attrs: None,
                            children: vec![quote_chain(levels)],
                            pos: None,
                        }]
                    },
                    pos: None,
                }],
            }))
        };
        let boundary = definition(MAX_CHECKED_AST_DEPTH - 4);
        let copied = boundary.try_clone().unwrap();
        assert_eq!(boundary.try_eq(&copied), Ok(true));
        assert_refused(definition(MAX_CHECKED_AST_DEPTH - 3));
    }
}

#[test]
fn checked_operations_on_a_small_stack() {
    const CHILD: &str = "CARVE_CHECKED_AST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &common::exact_test_name(module_path!(), "checked_operations_on_a_small_stack"),
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            result.status.success()
                && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let boundary = document(quote_chain(MAX_CHECKED_AST_DEPTH - 1));
            let copy = boundary.try_clone().unwrap();
            assert_eq!(boundary.try_eq(&copy), Ok(true));
            let inline_boundary = document(paragraph(inline_chain(MAX_CHECKED_AST_DEPTH - 2)));
            let copy = inline_boundary.try_clone().unwrap();
            assert_eq!(inline_boundary.try_eq(&copy), Ok(true));
            let mut mixed = quote_chain(3);
            for _ in 0..3 {
                let mut content = cell();
                content.blocks = Some(vec![mixed]);
                let mut target = table();
                target.rows.push(TableRow {
                    cells: vec![content],
                    attrs: None,
                    pos: None,
                });
                mixed = BlockNode::Figure(figure(FigureTarget::Table(target)));
            }
            let mixed = document(mixed);
            let copy = mixed.try_clone().unwrap();
            assert_eq!(mixed.try_eq(&copy), Ok(true));
            assert_refused(document(quote_chain(MAX_CHECKED_AST_DEPTH)));
            assert_refused(document(quote_chain(20_000)));
            assert_refused(document(paragraph(inline_chain(20_000))));
        })
        .unwrap()
        .join()
        .unwrap();
}
