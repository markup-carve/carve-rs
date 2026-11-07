#[test]
fn two_marked_blanks_keep_the_attached_items_heading() {
    let html = carve::to_html("> * +\n> ::: x\n>\n>\n>   # q\n");
    assert_eq!(html, "<blockquote>\n  <ul>\n    <li>\n      <div class=\"x\">\n\n      </div>\n      <h1 id=\"q\">q</h1>\n    </li>\n  </ul>\n</blockquote>");
}

#[test]
fn three_marked_blanks_keep_a_later_direct_child() {
    let html = carve::to_html("> - +\n> ::: x\n>\n>\n>\n>   # q\n");
    assert!(
        html.contains("</div>\n      <h1 id=\"q\">q</h1>\n    </li>"),
        "{html}"
    );
}
