use carve::to_html;

#[test]
fn a_reference_definition_ends_a_quoted_term_at_each_depth() {
    let cases = [
        (
            "> :: t\n> [r]: /u\ntail\n",
            "<blockquote>\n  <dl>\n    <dt>t</dt>\n  </dl>\n</blockquote>\n<p>tail</p>",
        ),
        (
            "> :: t\n>   [r]: /u\ntail\n",
            "<blockquote>\n  <dl>\n    <dt>t</dt>\n  </dl>\n</blockquote>\n<p>tail</p>",
        ),
        (
            "> > :: t\n> > [r]: /u\ntail\n",
            "<blockquote>\n  <blockquote>\n    <dl>\n      <dt>t</dt>\n    </dl>\n  </blockquote>\n</blockquote>\n<p>tail</p>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(to_html(source).trim(), expected, "{source:?}");
    }
}
