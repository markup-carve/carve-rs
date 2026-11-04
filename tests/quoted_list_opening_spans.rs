use carve::ast::{BlockNode, Pos};
fn verify(nodes: &[BlockNode], source: &[char], owner: usize, count: &mut usize) {
    fn opening(pos: &Pos, source: &[char], owner: usize, count: &mut usize) -> usize {
        let line_start = source[..pos.start_offset]
            .iter()
            .rposition(|c| *c == '\n' || *c == '\r')
            .map_or(0, |at| at + 1);
        assert_eq!(pos.start_column, owner, "{pos:?}");
        assert_eq!(pos.start_offset, line_start + owner - 1, "{pos:?}");
        let marker_at = pos.start_offset
            + source[pos.start_offset..]
                .iter()
                .take_while(|c| **c == ' ' || **c == '\t')
                .count();
        assert!(matches!(source[marker_at], '-' | '1'), "{pos:?}");
        *count += 1;
        marker_at - line_start + if source[marker_at] == '1' { 4 } else { 3 }
    }
    for node in nodes {
        match node {
            BlockNode::List(list) => {
                opening(list.pos.as_ref().unwrap(), source, owner, count);
                for item in &list.items {
                    let content = opening(item.pos.as_ref().unwrap(), source, owner, count);
                    verify(&item.children, source, content, count);
                }
            }
            BlockNode::BlockQuote(quote) => verify(
                &quote.children,
                source,
                quote.pos.as_ref().unwrap().start_column + 2,
                count,
            ),
            _ => {}
        }
    }
}
#[test]
fn quoted_list_spans_begin_at_owner_columns() {
    for depth in 1..=3 {
        for indent in [2, 3, 4, 6] {
            let prefix = "> ".repeat(depth);
            let source = format!(
                "{prefix}{}- a\n{prefix}- b\n{prefix}    - c\n{prefix}      :::\n1. lazy\n",
                " ".repeat(indent)
            );
            let options = carve::Options::default().with_positions(true);
            let doc = carve::parse_with_options(&source, &options);
            let mut count = 0;
            verify(
                &doc.children,
                &source.chars().collect::<Vec<_>>(),
                1,
                &mut count,
            );
            assert!(count >= 6, "{source}: {count}");
        }
    }
}
