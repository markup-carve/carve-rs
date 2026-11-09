// carve#5488: a fence-shaped verse line in a nested item's `::: |` stanza is
// payload (PART 0 FAMILIES line block, B-div-keep), not a fence opener, so a
// lazy line below it folds into the stanza (carve#4956).

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn a_fence_shaped_verse_line_keeps_a_nested_stanza_open_for_a_lazy_line() {
    check(
        "- x\n  - ::: |\n    ```\n  :\n",
        "<ul>\n  <li>x\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p><code>\n:</code></p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_lazy_line_after_fence_shaped_verse_stays_in_the_stanza_before_a_fence() {
    check(
        "- x\n  - ::: |\n    ```\n  :\n  ```\n",
        "<ul>\n  <li>x\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p><code>\n:</code></p>\n        </div>\n      </li>\n    </ul>\n    <pre><code></code></pre>\n  </li>\n</ul>",
    );
}

#[test]
fn a_quoted_nested_stanza_takes_the_lazy_line_carve_5488() {
    check(
        "> - x\n>   - ::: |\n>     ```\n>   :\n>   ```\n",
        "<blockquote>\n  <ul>\n    <li>x\n      <ul>\n        <li>\n          <div class=\"line-block\">\n            <p><code>\n:</code></p>\n          </div>\n        </li>\n      </ul>\n      <pre><code></code></pre>\n    </li>\n  </ul>\n</blockquote>",
    );
}

#[test]
fn a_tilde_verse_line_keeps_the_stanza_open() {
    check(
        "> - x\n>   - ::: |\n>     ```\n>   :\n>   ~~~\n",
        "<blockquote>\n  <ul>\n    <li>x\n      <ul>\n        <li>\n          <div class=\"line-block\">\n            <p><code>\n:</code></p>\n          </div>\n        </li>\n      </ul>\n      <pre><code></code></pre>\n    </li>\n  </ul>\n</blockquote>",
    );
}

#[test]
fn a_fence_below_the_stanza_host_still_interrupts() {
    check(
        "- x\n  - x\n    - ::: |\n      ```\n    ```\n    ````\n",
        "<ul>\n  <li>x\n    <ul>\n      <li>x\n        <ul>\n          <li>\n            <div class=\"line-block\">\n              <p><code></code></p>\n            </div>\n          </li>\n        </ul>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_verse_line_does_not_hold_a_code_fence_open_for_the_parent() {
    check(
        "- x\n  - x\n    - ::: |\n      ````\n     :\n     ~~~\n",
        "<ul>\n  <li>x\n    <ul>\n      <li>x\n        <ul>\n          <li>\n            <div class=\"line-block\">\n              <p><code>\n:</code></p>\n            </div>\n          </li>\n        </ul>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
}
