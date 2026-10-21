#[test]
fn a_marker_below_a_description_but_inside_its_item_opens_beside_the_definition_list() {
    let source = "- :: t\n  : para\n   - li\n";
    let expected = "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n      <dd>para</dd>\n    </dl>\n    <ul>\n      <li>li</li>\n    </ul>\n  </li>\n</ul>";
    for positions in [false, true] {
        let options = carve::Options {
            positions,
            ..Default::default()
        };
        assert_eq!(carve::to_html_with_options(source, &options), expected);
        let document = carve::parse_with_options(source, &options);
        let carve::BlockNode::List(outer) = &document.children[0] else {
            panic!("expected outer list");
        };
        assert!(matches!(
            outer.items[0].children.as_slice(),
            [
                carve::BlockNode::DefinitionList(_),
                carve::BlockNode::List(_)
            ]
        ));
    }
}
