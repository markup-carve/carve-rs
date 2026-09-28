use carve::ast::*;

fn terms(blocks: &[BlockNode]) -> Vec<&DefinitionTerm> {
    let mut found = Vec::new();
    for block in blocks {
        match block {
            BlockNode::DefinitionList(list) => {
                for item in &list.items {
                    found.extend(&item.terms);
                    for definition in &item.definitions {
                        found.extend(terms(&definition.children));
                    }
                }
            }
            BlockNode::BlockQuote(n) => found.extend(terms(&n.children)),
            BlockNode::List(n) => {
                for item in &n.items {
                    found.extend(terms(&item.children));
                }
            }
            BlockNode::Div(n) => found.extend(terms(&n.children)),
            _ => {}
        }
    }
    found
}

fn breaks(doc: &carve::Document) -> Vec<&Break> {
    terms(&doc.children)
        .into_iter()
        .flat_map(|term| &term.children)
        .filter_map(|node| match node {
            InlineNode::SoftBreak(br) => Some(br),
            _ => None,
        })
        .collect()
}

fn assert_gap(source: &str, pos: &Pos) {
    let chars: Vec<_> = source.chars().collect();
    let gap = &chars[pos.start_offset..pos.end_offset];
    assert_eq!(gap.iter().filter(|&&c| c == '\n').count(), 1);
    assert!(gap.iter().all(|c| c.is_whitespace() || *c == '>'));
}

#[test]
fn corpus_boundary_breaks_are_placed_and_existing_breaks_do_not_move() {
    let cases: &[(&str, &str)] = &[
        ("", "4-7, 1:5-2:3; 14-15, 2:10-3:1"),
        ("-2", "6-11, 1:7-2:5; 18-21, 2:12-3:3"),
        ("-3", "14-19, 3:7-4:5; 26-29, 4:12-5:3"),
        ("-4", "15-20, 3:7-4:5; 27-30, 4:12-5:3"),
        ("-5", "4-7, 1:5-2:3; 25-26, 4:6-5:1"),
        ("-6", "6-11, 1:7-2:5; 33-36, 4:8-5:3"),
        ("-7", "14-19, 3:7-4:5; 41-44, 6:8-7:3"),
        ("-8", "15-20, 3:7-4:5; 42-45, 6:8-7:3"),
        ("-22", "23-28, 5:7-6:5; 51-54, 9:8-10:3"),
        ("-24", "13-18, 3:7-4:5; 25-28, 4:12-5:3"),
        ("-9", "12-13, 3:5-4:1"),
        ("-10", "14-17, 3:7-4:3"),
        ("-11", "22-25, 5:7-6:3"),
        ("-12", "23-26, 5:7-6:3"),
        ("-13", "11-12, 3:5-4:1"),
        ("-14", "13-16, 3:7-4:3"),
        ("-15", "21-24, 5:7-6:3"),
        ("-16", "22-25, 5:7-6:3"),
        ("-23", "6-9, 1:7-2:3"),
        ("-25", "23-26, 5:7-6:3; 34-37, 6:11-7:3"),
        ("-17", ""),
        ("-18", ""),
        ("-19", ""),
        ("-20", ""),
        ("-21", ""),
    ];
    for (suffix, expected) in cases {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "tests/spec/tests/corpus/504-a-comment-or-a-definition-under-a-definition-term-folds-at-every-depth{suffix}.crv"
        ));
        let source = std::fs::read_to_string(path).expect("corpus fixture exists");
        let doc = carve::parse_with_options(&source, &carve::Options::new().with_positions(true));
        let spans: Vec<_> = breaks(&doc)
            .into_iter()
            .map(|br| {
                let pos = br.pos.as_ref().expect("term break has a position");
                assert_gap(&source, pos);
                format!(
                    "{}-{}, {}:{}-{}:{}",
                    pos.start_offset,
                    pos.end_offset,
                    pos.start_line,
                    pos.start_column,
                    pos.end_line,
                    pos.end_column
                )
            })
            .collect();
        assert_eq!(spans.join("; "), *expected, "fixture 504{suffix}");
    }
}

const MULTIPLE_PARTS: &str = ":: c\n  %% a\n  %% b\n  more\n";

fn neighbour_pos(node: &InlineNode) -> &Pos {
    match node {
        InlineNode::Text(n) => n.pos.as_ref(),
        InlineNode::Comment(n) => n.pos.as_ref(),
        other => panic!("unexpected neighbour: {other:?}"),
    }
    .expect("neighbour has a position")
}

#[test]
fn each_boundary_uses_its_own_neighbours() {
    for source in [MULTIPLE_PARTS, ":: café\n  %% α\n  %% β\n  more\n"] {
        let doc = carve::parse_with_options(source, &carve::Options::new().with_positions(true));
        assert_eq!(breaks(&doc).len(), 3);
        for term in terms(&doc.children) {
            for (index, node) in term.children.iter().enumerate() {
                if let InlineNode::SoftBreak(br) = node {
                    let prev = neighbour_pos(&term.children[index - 1]);
                    let next = neighbour_pos(&term.children[index + 1]);
                    let expected = Pos {
                        start_line: prev.end_line,
                        end_line: next.start_line,
                        start_column: prev.end_column,
                        end_column: next.start_column,
                        start_offset: prev.end_offset,
                        end_offset: next.start_offset,
                        file: prev.file.clone(),
                    };
                    assert_eq!(br.pos.as_ref(), Some(&expected));
                    assert_gap(source, &expected);
                }
            }
        }
    }
}

#[test]
fn disabling_positions_keeps_the_breaks_unplaced() {
    let doc =
        carve::parse_with_options(MULTIPLE_PARTS, &carve::Options::new().with_positions(false));
    let found = breaks(&doc);
    assert_eq!(found.len(), 3);
    assert!(found.iter().all(|br| br.pos.is_none()));
}
