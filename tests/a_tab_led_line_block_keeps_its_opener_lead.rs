use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn a_tab_led_closer_in_a_nested_item_closes_its_line_block() {
    check(
        "- a\n  - a\n  \t::: |\n  \t:::\n  \tx\n",
        "<ul>\n  <li>a\n    <ul>\n      <li>a\n        <div class=\"line-block\">\n\n        </div>\n        x\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_tab_led_payload_line_is_measured_from_its_opener() {
    check(
        "- a\n  - a\n  \t::: |\n  \t::: >\n  \t:::\n  \tx\nb\n",
        "<ul>\n  <li>a\n    <ul>\n      <li>a\n        <div class=\"line-block\">\n          <p>::: &gt;</p>\n        </div>\n        x\nb\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_tab_past_a_spaced_opener_stays_verse_indentation() {
    check(
        "- a\n  ::: |\n  \ty\n  :::\n",
        "<ul>\n  <li>a\n    <div class=\"line-block\">\n      <p>&nbsp;&nbsp;&nbsp;&nbsp;y</p>\n    </div>\n  </li>\n</ul>",
    );
}
