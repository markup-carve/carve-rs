// carve#5720: a quoted item body keeps a tab's authored remainder when a
// nested frame strips it (PART 9 §24 C5), as an unquoted body does.

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source).trim_end(), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned.trim_end(), expected);
}

#[test]
fn a_heading_on_an_off_stop_tab_run_opens_in_a_quoted_item() {
    check(
        "> - a\n>   - a\n> \t- a\n> \t  {.x}\n> \ta\n>    \t# h\n",
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>a\n          <ul>\n            <li>a</li>\n          </ul>\n          a\n          <h1 id=\"h\">h</h1>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>",
    );
}

#[test]
fn a_quote_on_an_off_stop_tab_run_opens_in_a_quoted_item() {
    check(
        "> - a\n>   - a\n>  \t- a\n>     \n>     a\n>   \t> z\n",
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li><p>a</p>\n          <ul>\n            <li>a</li>\n          </ul>\n          <p>a</p>\n          <blockquote><p>z</p></blockquote>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>",
    );
}

#[test]
fn a_space_led_ladder_keeps_the_tab_remainder_in_a_quote() {
    check(
        "> - a\n>   - a\n>     - a\n>       {.x}\n>     a\n>   \t# h\n",
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>a\n          <ul>\n            <li>a</li>\n          </ul>\n          a\n          <h1 id=\"h\">h</h1>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>",
    );
}
