use carve::ast::BlockNode;
use carve::Options;

fn starts(input: &str) -> Vec<(usize, usize)> {
    let doc = carve::parse_with_options(input, &Options::default().with_positions(true));
    let BlockNode::List(list) = &doc.children[0] else {
        panic!("expected list: {input:?}")
    };
    let mut result = vec![];
    let pos = list.pos.as_ref().unwrap();
    result.push((pos.start_offset, pos.start_column));
    for item in &list.items {
        let pos = item.pos.as_ref().unwrap();
        result.push((pos.start_offset, pos.start_column));
        for child in &item.children {
            if let BlockNode::List(inner) = child {
                let pos = inner.pos.as_ref().unwrap();
                result.push((pos.start_offset, pos.start_column));
                for item in &inner.items {
                    let pos = item.pos.as_ref().unwrap();
                    result.push((pos.start_offset, pos.start_column));
                }
            }
        }
    }
    result
}

#[test]
fn list_and_item_spans_include_placing_indent() {
    assert_eq!(starts("  - a\n  - b\n"), [(0, 1), (0, 1), (6, 1)]);
    assert_eq!(starts("- a\n   - b\n"), [(0, 1), (0, 1), (6, 3), (6, 3)]);
}

#[test]
fn footnote_list_starts_at_footnote_content_column() {
    let source = "x[^a]\n\n[^a]: note\n   - a\n";
    let doc = carve::parse_with_options(source, &Options::default().with_positions(true));
    let blocks = doc.footnote_defs.get("a").unwrap();
    let BlockNode::List(list) = &blocks[1] else {
        panic!("expected list in footnote")
    };
    for pos in [
        list.pos.as_ref().unwrap(),
        list.items[0].pos.as_ref().unwrap(),
    ] {
        assert_eq!((pos.start_offset, pos.start_column), (20, 3));
    }
}

#[test]
fn description_list_starts_at_description_content_column() {
    let source = ":: term\n:  - a\n";
    let doc = carve::parse_with_options(source, &Options::default().with_positions(true));
    let BlockNode::DefinitionList(defs) = &doc.children[0] else {
        panic!("expected definition list")
    };
    let BlockNode::List(list) = &defs.items[0].definitions[0].children[0] else {
        panic!("expected list in description")
    };
    for pos in [
        list.pos.as_ref().unwrap(),
        list.items[0].pos.as_ref().unwrap(),
    ] {
        assert_eq!((pos.start_offset, pos.start_column), (11, 4));
    }
}

#[test]
fn lone_carriage_returns_keep_list_owner_columns() {
    let source = "a\r\r  - b\r";
    let doc = carve::parse_with_options(source, &Options::default().with_positions(true));
    let BlockNode::List(list) = &doc.children[1] else {
        panic!("expected list")
    };
    for pos in [
        list.pos.as_ref().unwrap(),
        list.items[0].pos.as_ref().unwrap(),
    ] {
        assert_eq!((pos.start_offset, pos.start_column), (3, 1));
    }
}

#[test]
fn first_line_bom_places_indented_list_after_the_bom() {
    let source = "\u{feff}  - a\n";
    let doc = carve::parse_with_options(source, &Options::default().with_positions(true));
    let BlockNode::List(list) = &doc.children[0] else {
        panic!("expected list")
    };
    for pos in [
        list.pos.as_ref().unwrap(),
        list.items[0].pos.as_ref().unwrap(),
    ] {
        assert_eq!((pos.start_offset, pos.start_column), (1, 2));
    }
}

#[test]
fn shifted_quote_prefix_uses_the_list_opening_line() {
    let source = " > para\n>\n>   - b\n";
    let doc = carve::parse_with_options(source, &Options::default().with_positions(true));
    let BlockNode::BlockQuote(quote) = &doc.children[0] else {
        panic!("expected quote")
    };
    let BlockNode::List(list) = &quote.children[1] else {
        panic!("expected list")
    };
    for pos in [
        list.pos.as_ref().unwrap(),
        list.items[0].pos.as_ref().unwrap(),
    ] {
        assert_eq!((pos.start_offset, pos.start_column), (12, 3));
    }
}

#[test]
fn quote_after_list_marker_owns_its_placing_indent() {
    let source = "> - >  - a\n";
    let doc = carve::parse_with_options(source, &Options::default().with_positions(true));
    let BlockNode::BlockQuote(outer_quote) = &doc.children[0] else {
        panic!("expected outer quote")
    };
    let BlockNode::List(outer_list) = &outer_quote.children[0] else {
        panic!("expected outer list")
    };
    let BlockNode::BlockQuote(inner_quote) = &outer_list.items[0].children[0] else {
        panic!("expected inner quote")
    };
    let BlockNode::List(inner_list) = &inner_quote.children[0] else {
        panic!("expected inner list")
    };
    for pos in [
        inner_list.pos.as_ref().unwrap(),
        inner_list.items[0].pos.as_ref().unwrap(),
    ] {
        assert_eq!((pos.start_offset, pos.start_column), (6, 7));
    }
}
