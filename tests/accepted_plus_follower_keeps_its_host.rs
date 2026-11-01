use carve::to_html;

#[test]
fn an_indented_quote_prefix_does_not_move_the_follower_out_of_its_item() {
    let source = "> - - - +\n  >   x\n";
    let html = to_html(source);
    assert!(html.contains("<li>x</li>"), "{html}");
    assert!(!html.contains("<p>x</p>"), "{html}");
}

#[test]
fn the_shallower_follower_keeps_the_same_owner() {
    let source = "> - - +\n  > x\n";
    let html = to_html(source);
    assert!(html.contains("<li>x</li>"), "{html}");
}
