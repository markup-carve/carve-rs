use carve::to_html;

#[test]
fn a_document_abbreviation_ends_a_lazy_quote() {
    let source = "-  - > q\ntail\n*[A]: b\ntail\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <blockquote><p>q\ntail</p></blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>tail</p>";
    assert_eq!(to_html(source).trim(), expected);
}

#[test]
fn a_definition_below_an_item_content_column_stands_in_the_document() {
    let source = "-  n\n  *[A]: b\n\nA\n";
    let expected = "<ul>\n  <li>n</li>\n</ul>\n<p><abbr title=\"b\">A</abbr></p>";
    assert_eq!(to_html(source).trim(), expected);
}

#[test]
fn an_abbreviation_spelling_inside_a_container_stays_text() {
    let cases = [
        (
            "::: box\n*[A]: Alpha\nA\n:::\n",
            "<div class=\"box\">\n  <p>*[A]: Alpha\nA</p>\n</div>",
        ),
        (
            "- a\n  *[A]: Alpha\nA\n",
            "<ul>\n  <li>a\n*[A]: Alpha\nA</li>\n</ul>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(to_html(source).trim(), expected, "{source:?}");
    }
}
