#[test]
fn an_indented_quoted_item_keeps_plain_lines_at_its_base() {
    for (source, expected) in [
        (
            " - > a\n b",
            "<ul>\n  <li>\n    <blockquote><p>a\nb</p></blockquote>\n  </li>\n</ul>",
        ),
        (
            "  - > a\n  b",
            "<ul>\n  <li>\n    <blockquote><p>a\nb</p></blockquote>\n  </li>\n</ul>",
        ),
        (
            " - > a\n b\n c",
            "<ul>\n  <li>\n    <blockquote><p>a\nb\nc</p></blockquote>\n  </li>\n</ul>",
        ),
        (
            " - > a\n - c",
            "<ul>\n  <li>\n    <blockquote><p>a</p></blockquote>\n  </li>\n  <li>c</li>\n</ul>",
        ),
    ] {
        assert_eq!(
            carve::to_html(source).trim(),
            expected,
            "source: {source:?}"
        );
    }
}
