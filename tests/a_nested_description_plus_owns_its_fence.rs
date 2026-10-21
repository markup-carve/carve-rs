use carve::ast::BlockNode;

const SOURCE: &str = ":: t\n:  d\n   :: term\n   :  detail\n   +\n   ```\n   code\n   ```\n";

#[test]
fn the_inner_description_owns_the_fence_after_its_plus() {
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>d</p>\n    <dl>\n      <dt>term</dt>\n      <dd>\n        <p>detail</p>\n        <pre><code>code\n</code></pre>\n      </dd>\n    </dl>\n  </dd>\n</dl>";
    assert_eq!(carve::to_html(SOURCE), expected);

    let doc = carve::parse_with_options(
        SOURCE,
        &carve::Options {
            positions: true,
            ..Default::default()
        },
    );
    let BlockNode::DefinitionList(outer) = &doc.children[0] else {
        panic!("expected outer definition list");
    };
    let BlockNode::DefinitionList(inner) = &outer.items[0].definitions[0].children[1] else {
        panic!("expected inner definition list");
    };
    let description = &inner.items[0].definitions[0];
    assert!(matches!(
        description.children.as_slice(),
        [BlockNode::Paragraph(_), BlockNode::CodeBlock(_)]
    ));
    let BlockNode::CodeBlock(code) = &description.children[1] else {
        unreachable!();
    };
    let pos = code.pos.as_ref().expect("fence must have source position");
    assert_eq!((pos.start_line, pos.end_line), (6, 8));
    assert_eq!((pos.start_column, pos.end_column), (4, 7));
}

#[test]
fn outer_and_wrong_column_markers_do_not_move_a_fence_into_the_inner_description() {
    for source in [
        ":: t\n:  d\n   :: term\n   :  detail\n+\n   ```\n   code\n   ```\n",
        ":: t\n:  d\n   :: term\n   :  detail\n  +\n   ```\n   code\n   ```\n",
    ] {
        let html = carve::to_html(source);
        assert!(
            html.contains("</dl>\n    <pre><code>code\n</code></pre>"),
            "{html}"
        );
    }
}
