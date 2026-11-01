#[test]
fn closed_heading_attachment_does_not_take_a_column_zero_tail() {
    let cases = [
        (
            "- - +\n  # h\nafter\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <h1 id=\"h\">h</h1>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>after</p>",
        ),
        (
            "- d\n  - +\n  # h\nafter\n",
            "<ul>\n  <li>d\n    <ul>\n      <li>\n        <h1 id=\"h\">h</h1>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>after</p>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(carve::to_html(source), expected, "{source}");
    }
}

#[test]
fn open_paragraph_attachment_keeps_its_lazy_tail() {
    assert_eq!(
        carve::to_html("- - +\n  paragraph\nafter\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>paragraph\nafter</li>\n    </ul>\n  </li>\n</ul>"
    );
}
