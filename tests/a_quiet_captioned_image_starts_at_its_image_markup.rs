use carve::ast::{BlockNode, FigureTarget};

#[test]
fn a_quiet_prefix_does_not_move_the_image_targets_start() {
    let source = "{%%}![a](/i)\n^ cap\n";
    let doc = carve::parse_with_options(source, &carve::Options::default().with_positions(true));
    let BlockNode::Figure(figure) = &doc.children[0] else {
        panic!("expected figure")
    };
    let FigureTarget::Image(image) = &*figure.target else {
        panic!("expected image target")
    };
    let image_pos = image.pos.as_ref().expect("image position");
    assert_eq!(
        &source[image_pos.start_offset..image_pos.end_offset],
        "![a](/i)"
    );
    assert_eq!(
        figure.pos.as_ref().expect("figure position").start_offset,
        0
    );
}
