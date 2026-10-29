#[test]
fn refused_plus_after_indented_list_keeps_later_markers_in_paragraph() {
    for (tail, expected_tail) in [
        ("- s", "<ul>\n  <li>s</li>\n</ul>"),
        ("- s\n- t", "<ul>\n  <li>s</li>\n  <li>t</li>\n</ul>"),
    ] {
        let source = format!(" - >\n+\n{tail}");
        let expected = format!(
            "<ul>\n  <li>\n    <blockquote>\n\n    </blockquote>\n  </li>\n</ul>\n<p>+</p>\n{expected_tail}"
        );
        assert_eq!(carve::to_html(&source), expected, "{source}");
    }
}
