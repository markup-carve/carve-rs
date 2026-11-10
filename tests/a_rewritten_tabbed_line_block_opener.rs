use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn a_tabbed_opener_rewritten_at_its_parent_keeps_payload_lead() {
    check(
        "- a\n  - a\n\t ::: |\n  \tx *b*\n\t :::\n",
        "<ul>\n  <li>a\n    <ul>\n      <li>a\n        <div class=\"line-block\">\n          <p>&nbsp;&nbsp;x <strong>b</strong></p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_rewritten_opener_keeps_the_same_lead_at_three_item_depths() {
    check(
        "* a\n  * a\n    * a\n\t ::: |\n  \tx *b*\n\t :::\n",
        "<ul>\n  <li>a\n    <ul>\n      <li>a\n        <ul>\n          <li>a</li>\n        </ul>\n        <div class=\"line-block\">\n          <p>&nbsp;&nbsp;x <strong>b</strong></p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_quote_owned_rewritten_opener_uses_its_authored_cut() {
    check(
        "> * a\n>   * a\n>     \t::: |\n> \t\talpha\n>     \t:::\n",
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>a\n          <div class=\"line-block\">\n            <p>alpha</p>\n          </div>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>",
    );
}
