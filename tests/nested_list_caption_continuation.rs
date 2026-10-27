#[test]
fn caption_after_marker_line_sublist_keeps_plain_tail() {
    assert_eq!(
        carve::to_html("- - x\n  > q\n  ^ c\nt"),
        "<ul>\n  <li>\n    <ul>\n      <li>x</li>\n    </ul>\n    <figure>\n      <blockquote><p>q</p></blockquote>\n      <figcaption>c\nt</figcaption>\n    </figure>\n  </li>\n</ul>"
    );
}
