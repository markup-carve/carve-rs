//! A verse span blank or payload inside list items (CARVE-P9-047, PART 9 §17 L1).
//! The expected HTML is the spec model's.

fn html(source: &str) -> String {
    carve::to_html(source).trim_end().to_string()
}

/// carve#5269
#[test]
fn a_nested_quoted_item_releases_its_follower_to_the_quote() {
    assert_eq!(
        html("> - a\n>   - ::: |\n>     x\n>     %%%\n>     \nb\n"),
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>\n          <div class=\"line-block\">\n            <p>x</p>\n          </div>\n        </li>\n      </ul>\n    </li>\n  </ul>\n  <p>b</p>\n</blockquote>"
    );
}

/// carve#5269
#[test]
fn a_nested_item_span_blank_ends_the_list() {
    assert_eq!(
        html("- a\n  - ::: |\n    x\n    %%%\n    \nb\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p>x</p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>b</p>"
    );
}

/// carve#5269
#[test]
fn an_item_span_blank_ends_the_item_at_document_level() {
    assert_eq!(
        html("- ::: |\n  x\n  %%%\n  \nb\n"),
        "<ul>\n  <li>\n    <div class=\"line-block\">\n      <p>x</p>\n    </div>\n  </li>\n</ul>\n<p>b</p>"
    );
}

/// carve#5269
#[test]
fn a_third_level_quoted_item_releases_its_follower_to_the_quote() {
    assert_eq!(
        html("> - a\n>   - b\n>     - ::: |\n>       x\n>       %%%\n>       \nc\n"),
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>b\n          <ul>\n            <li>\n              <div class=\"line-block\">\n                <p>x</p>\n              </div>\n            </li>\n          </ul>\n        </li>\n      </ul>\n    </li>\n  </ul>\n  <p>c</p>\n</blockquote>"
    );
}

/// carve#5269
#[test]
fn a_quoted_item_own_body_stanza_takes_the_follower() {
    assert_eq!(
        html("> - a\n>   ::: |\n>   x\n>   %%%\n>   \nb\n"),
        "<blockquote>\n  <ul>\n    <li>a\n      <div class=\"line-block\">\n        <p>x<br>\nb</p>\n      </div>\n    </li>\n  </ul>\n</blockquote>"
    );
}

/// carve#5269
#[test]
fn a_quoted_item_stanza_takes_an_unmarked_description_follower() {
    assert_eq!(
        html("> - ::: |\n>   x\n>   %%%\n>\n: b\n"),
        "<blockquote>\n  <ul>\n    <li>\n      <div class=\"line-block\">\n        <p>x<br>\n: b</p>\n      </div>\n    </li>\n  </ul>\n</blockquote>"
    );
}

/// carve#5266
#[test]
fn an_unclosed_span_payload_keeps_the_prior_blank_before_a_sibling() {
    assert_eq!(
        html("- ::: |\n  x\n  \n  %%%\n  hidden\n- b\n"),
        "<ul>\n  <li>\n    <div class=\"line-block\">\n      <p>x</p>\n    </div>\n  </li>\n  <li><p>b</p></li>\n</ul>"
    );
}

/// carve#5266
#[test]
fn a_nested_unclosed_span_payload_keeps_the_prior_blank() {
    assert_eq!(
        html("- a\n  - ::: |\n    x\n\n    %%%\n    hidden\n  - b\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p>x</p>\n        </div>\n      </li>\n      <li><p>b</p></li>\n    </ul>\n  </li>\n</ul>"
    );
}

/// carve#5266
#[test]
fn a_marker_line_span_payload_stays_interior() {
    assert_eq!(
        html("- %%%\n  x\n\n  %%%\n- b\n"),
        "<ul>\n  <li></li>\n  <li>b</li>\n</ul>"
    );
}

/// carve#5269
#[test]
fn a_quoted_item_stanza_leaves_a_quoted_follower_to_the_quote() {
    assert_eq!(
        html("> - ::: |\n>   x\n>   %%%\n>\n> b\n"),
        "<blockquote>\n  <ul>\n    <li>\n      <div class=\"line-block\">\n        <p>x</p>\n      </div>\n    </li>\n  </ul>\n  <p>b</p>\n</blockquote>"
    );
}

/// carve#5269
#[test]
fn a_quoted_item_description_stanza_ends_at_the_span_blank() {
    assert_eq!(
        html("> - :: t\n>   : ::: |\n>     x\n>     %%%\n>\n: b\n"),
        "<blockquote>\n  <ul>\n    <li>\n      <dl>\n        <dt>t</dt>\n        <dd>\n          <div class=\"line-block\">\n            <p>x</p>\n          </div>\n        </dd>\n      </dl>\n    </li>\n  </ul>\n  <p>: b</p>\n</blockquote>"
    );
}

/// carve#5266
#[test]
fn a_code_fence_percent_line_opens_no_span() {
    assert_eq!(
        html("- ```\n  %%%\n  ```\n  x\n\n  %%%\n  h\n- b\n"),
        "<ul>\n  <li>\n    <pre><code>%%%\n</code></pre>\n    <p>x</p>\n  </li>\n  <li><p>b</p></li>\n</ul>"
    );
}

/// carve#5273
#[test]
fn an_unquoted_description_marker_after_a_span_opens_a_sibling() {
    assert_eq!(
        html(":: t\n: ::: |\n  x\n  %%%\n\n: b\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <div class=\"line-block\">\n      <p>x</p>\n    </div>\n  </dd>\n  <dd>b</dd>\n</dl>"
    );
}

/// carve#5273
#[test]
fn an_unquoted_description_marker_after_an_item_span_opens_a_sibling() {
    assert_eq!(
        html(":: t\n: - ::: |\n    x\n    %%%\n: b\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p>x</p>\n        </div>\n      </li>\n    </ul>\n  </dd>\n  <dd>b</dd>\n</dl>"
    );
}

/// carve#5266
#[test]
fn a_fence_shaped_verse_line_does_not_hide_the_span() {
    assert_eq!(
        html("- ::: |\n  ```\n  \n  %%%\n  hidden\n- b\n"),
        "<ul>\n  <li>\n    <div class=\"line-block\">\n      <p><code></code></p>\n    </div>\n  </li>\n  <li><p>b</p></li>\n</ul>"
    );
}

/// carve#5266
#[test]
fn an_empty_stanza_under_an_unclosed_span_takes_no_description_lazy_line() {
    assert_eq!(
        html(":: t\n: - ::: |\n    x\n\n    %%%\n    hidden\nb\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p>x</p>\n        </div>\n      </li>\n    </ul>\n  </dd>\n</dl>\n<p>b</p>"
    );
}
