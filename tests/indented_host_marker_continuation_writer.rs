use carve::{to_carve, to_html};

#[test]
fn a_marker_line_child_inside_a_div_keeps_its_following_blocks() {
    let source = include_str!(
        "spec/tests/corpus/743-a-later-term-after-a-shifted-separator-opens-in-the-outer-item-3.crv"
    );
    // The row's golden at spec c3c2f80f; the 5889a6fe golden waits on markup-carve/carve#4861.
    let expected = "<div class=\"box\">\n  <ul>\n    <li>\n      <ul>\n        <li>\n          <dl>\n            <dt>t</dt>\n          </dl>\n        </li>\n      </ul>\n      : a\n      <dl>\n        <dt>sibling\ntail</dt>\n      </dl>\n    </li>\n  </ul>\n</div>";
    let formatted = to_carve(source);
    assert_eq!(to_html(source).trim(), expected.trim());
    assert_eq!(to_html(&formatted), to_html(source));
    assert_eq!(to_carve(&formatted), formatted);
}
