use carve::{parse, render_carve, render_html, BlockNode};

#[test]
fn table_width_precision_preserves_decimal_percentages() {
    let doc = parse("{widths=33.3,0.7,7}\n| a | b | c |\n");
    let BlockNode::Table(table) = &doc.children[0] else {
        panic!("not a table")
    };
    assert_eq!(
        table.columns.iter().map(|c| c.width).collect::<Vec<_>>(),
        vec![Some(0.333), Some(0.007), Some(0.07)]
    );
    let html = render_html(&doc).unwrap();
    for value in ["33.3", "0.7", "7"] {
        assert!(html.contains(&format!("width: {value}%;")), "{html}");
    }
}

#[test]
fn table_width_precision_preserves_imported_fractions() {
    for width in [0.013, 1.0 / 3.0, 1e-20, f64::from_bits(1)] {
        let mut doc = parse("| a |\n");
        let BlockNode::Table(table) = &mut doc.children[0] else {
            panic!("not a table")
        };
        table.columns = vec![carve::TableColumn {
            width: Some(width),
            align: None,
            valign: None,
        }];
        let source = render_carve(&doc).unwrap();
        let reparsed = parse(&source);
        let BlockNode::Table(table) = &reparsed.children[0] else {
            panic!("not a table")
        };
        assert_eq!(table.columns[0].width, Some(width), "{source}");
    }
}
