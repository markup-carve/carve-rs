use carve::lint_carve;
use carve::{parse, BlockNode, TableAlign, TableVerticalAlign};

#[test]
fn positional_attributes_populate_the_table_model() {
    let doc = parse("{aligns=right valigns=top widths=25}\n| A |\n");
    let BlockNode::Table(table) = &doc.children[0] else {
        panic!("not a table")
    };
    assert_eq!(table.columns.len(), 1);
    assert_eq!(table.columns[0].align, Some(TableAlign::Right));
    assert_eq!(table.columns[0].valign, Some(TableVerticalAlign::Top));
    assert_eq!(table.columns[0].width, Some(0.25));
}

#[test]
fn table_column_rules_are_reported() {
    let source = "{aligns=left valigns=top widths=60,50}\n|=^ A | B |\n|<tight | x |\n";
    let rules: Vec<_> = lint_carve(source).into_iter().map(|w| w.rule).collect();
    assert!(rules.contains(&"table-column-arity"), "{rules:?}");
    assert!(rules.contains(&"table-column-overlap"), "{rules:?}");
    assert!(rules.contains(&"table-width-total"), "{rules:?}");
    assert!(rules.contains(&"table-alignment-run-padding"), "{rules:?}");
}
