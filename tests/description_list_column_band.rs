#[test]
fn a_blank_separated_marker_below_the_item_column_starts_a_sibling_list() {
    let cases = [
        (
            ":: term\n: - a\n\n   - c",
            "<dl>\n  <dt>term</dt>\n  <dd>\n    <ul>\n      <li><p>a</p></li>\n      <li><p>c</p></li>\n    </ul>\n  </dd>\n</dl>",
        ),
        (
            ":: term\n: - a\n\n   - c\n   - d",
            "<dl>\n  <dt>term</dt>\n  <dd>\n    <ul>\n      <li><p>a</p></li>\n      <li><p>c</p></li>\n      <li><p>d</p></li>\n    </ul>\n  </dd>\n</dl>",
        ),
        (
            ":: term\n: - a\n\n   - c\n\n   - d",
            "<dl>\n  <dt>term</dt>\n  <dd>\n    <ul>\n      <li><p>a</p></li>\n      <li><p>c</p></li>\n      <li><p>d</p></li>\n    </ul>\n  </dd>\n</dl>",
        ),
        (
            ":: term\n: - a\n\n    - c",
            "<dl>\n  <dt>term</dt>\n  <dd>\n    <ul>\n      <li>a\n        <ul>\n          <li>c</li>\n        </ul>\n      </li>\n    </ul>\n  </dd>\n</dl>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(
            carve::to_html(source).trim(),
            expected,
            "source: {source:?}"
        );
    }
}
