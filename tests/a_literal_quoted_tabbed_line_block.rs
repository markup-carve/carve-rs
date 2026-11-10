use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn a_one_column_verse_line_under_a_literal_tabbed_opener_uses_frame_lead() {
    check(
        "> - a\n>   - a\n>   \t::: |\n> \t z\n>   \t:::\n",
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>a\n          <div class=\"line-block\">\n            <p>z</p>\n          </div>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>",
    );
}

#[test]
fn a_blank_before_the_verse_line_keeps_the_same_column() {
    check(
        "> - a\n>   - a\n>   \t::: |\n>\n> \t z\n>   \t:::\n",
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>a\n          <div class=\"line-block\">\n            <p>z</p>\n          </div>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>",
    );
}

#[test]
fn sibling_verse_lines_each_keep_their_column() {
    check(
        "> - a\n>   - a\n>   \t::: |\n> \t x\n> \t z\n>   \t:::\n",
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>a\n          <div class=\"line-block\">\n            <p>x<br>\nz</p>\n          </div>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>",
    );
}

#[test]
fn a_tab_after_one_space_crosses_the_quoted_item_column() {
    check(
        "> - a\n>   - a\n>   \t::: |\n>  \t z\n>   \t:::\n",
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>a\n          <div class=\"line-block\">\n            <p>z</p>\n          </div>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>",
    );
}

#[test]
fn a_later_verse_line_keeps_its_column_after_plain_text() {
    check(
        "> - a\n>   - a\n>   \t::: |\n>   \tfirst\n> \t z\n>   \t:::\n",
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>a\n          <div class=\"line-block\">\n            <p>first<br>\nz</p>\n          </div>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>",
    );
}
