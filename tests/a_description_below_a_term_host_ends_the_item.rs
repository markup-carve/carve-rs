use carve::to_html;

#[test]
fn description_below_a_term_in_an_unclosed_div_leaves_the_item() {
    let rows = [
        (
            "- a\n  ::: box\n  :: sub\n: def\n",
            "<ul>\n  <li>a\n    <div class=\"box\">\n      <dl>\n        <dt>sub</dt>\n      </dl>\n    </div>\n  </li>\n</ul>\n<p>: def</p>\n",
        ),
        (
            "- a\n  - b\n    ::: box\n    :: sub\n: def\n",
            "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <div class=\"box\">\n          <dl>\n            <dt>sub</dt>\n          </dl>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>: def</p>\n",
        ),
    ];
    for (source, expected) in rows {
        assert_eq!(
            to_html(source).trim_end_matches('\n'),
            expected.trim_end_matches('\n'),
            "{source}"
        );
    }
}
