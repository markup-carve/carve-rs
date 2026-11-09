//! A marker-line plus grants one list block. A later consumed plus belongs to
//! the host, while a blank in a child fence stays with the attached list.

#[test]
fn later_plus_does_not_extend_note_attached_list() {
    let source = "[^a]: +\n- li\n+\n 1. sib\n\nsee[^a]\n";
    let html = carve::to_html(source);
    assert!(
        html.starts_with("<ol>\n  <li>sib</li>\n</ol>\n<p>see"),
        "{html}"
    );
    assert!(
        html.contains("<ul>\n        <li>li</li>\n      </ul>"),
        "{html}"
    );
    assert!(
        !html.contains("<li id=\"fn1\">\n      <ul>\n        <li>li</li>\n      </ul>\n      <ol>"),
        "{html}"
    );
}
