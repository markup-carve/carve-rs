use carve::to_html;

#[test]
fn nested_colon_div_ends_before_attached_tail() {
    let cases = [
        ("- - ::: d\n  +\ntail", "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"d\">\n\n        </div>\n        tail\n      </li>\n    </ul>\n  </li>\n</ul>"),
        ("- - - ::: d\n    +\ntail", "<ul>\n  <li>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <div class=\"d\">\n\n            </div>\n            tail\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>"),
    ];
    for (source, expected) in cases {
        assert_eq!(to_html(source), expected, "{source:?}");
    }
}
