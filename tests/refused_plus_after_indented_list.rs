#[test]
fn refused_plus_after_indented_list_keeps_later_markers_in_paragraph() {
    for (tail, expected_tail) in [("- s", "+\n- s"), ("- s\n- t", "+\n- s\n- t")] {
        let source = format!(" - >\n+\n{tail}");
        let expected = format!(
            "<ul>\n  <li>\n    <blockquote>\n\n    </blockquote>\n  </li>\n</ul>\n<p>{expected_tail}</p>"
        );
        assert_eq!(carve::to_html(&source), expected, "{source}");
    }
}
