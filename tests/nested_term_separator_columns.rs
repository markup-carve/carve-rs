#[test]
fn descriptions_below_an_authored_nested_term_follow_their_columns() {
    let cases = [
        (
            "- a\n    :: t\n: d\n",
            "<ul>\n  <li>a\n    <dl>\n      <dt>t</dt>\n    </dl>\n  </li>\n</ul>\n<p>: d</p>",
        ),
        (
            "- a\n    :: t\n  : d\n",
            "<ul>\n  <li>a\n    <dl>\n      <dt>t</dt>\n      <dd>d</dd>\n    </dl>\n  </li>\n</ul>",
        ),
        (
            "- a\n    :: t\n    : d\n",
            "<ul>\n  <li>a\n    <dl>\n      <dt>t</dt>\n      <dd>d</dd>\n    </dl>\n  </li>\n</ul>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(carve::to_html(source), expected, "{source:?}");
    }
}
