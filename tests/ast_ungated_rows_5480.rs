use carve::ast::{BlockNode, InlineNode};

fn parse(source: &str) -> carve::ast::Document {
    carve::parse_with_options(
        source,
        &carve::Options {
            positions: true,
            ..Default::default()
        },
    )
}

#[test]
fn expanded_tab_columns_have_no_source_position() {
    let source = "> - a\n>   - a\n>   \t::: |\n> \t\tz\n>   \t:::\n";
    let doc = parse(source);
    let mut spaces = Vec::new();
    fn walk(blocks: &[BlockNode], spaces: &mut Vec<Option<carve::ast::Pos>>) {
        for block in blocks {
            match block {
                BlockNode::BlockQuote(q) => walk(&q.children, spaces),
                BlockNode::List(l) => {
                    for item in &l.items {
                        walk(&item.children, spaces);
                    }
                }
                BlockNode::LineBlock(v) => walk(&v.children, spaces),
                BlockNode::Paragraph(p) => {
                    for child in &p.children {
                        if let InlineNode::NonBreakingSpace(n) = child {
                            spaces.push(n.pos.clone());
                        }
                    }
                }
                _ => {}
            }
        }
    }
    walk(&doc.children, &mut spaces);
    assert_eq!(spaces.len(), 2);
    assert!(spaces.iter().all(Option::is_none), "{spaces:?}");
}

#[test]
fn earlier_nested_definition_position_wins_over_losing_duplicate() {
    let source = "[^n]: +\n[^m]: d1\n[^m]: +\n\n  para\n\nsee[^n][^m]\n";
    let doc = parse(source);
    let pos = doc.footnote_def_pos.get("m").expect("m position");
    assert_eq!((pos.start_offset, pos.end_offset), (8, 16));
}
