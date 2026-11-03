//! A quote `+` that attaches an item opening an unclosed code fence adds no
//! separator line inside that fence (CARVE-P9-067, carve#5051).

use carve::to_html;

const EMPTY_JS: &str = "<blockquote>\n  <ul>\n    <li>\n      <pre><code class=\"language-js\"></code></pre>\n    </li>\n  </ul>\n  <p>q</p>\n</blockquote>";

#[test]
fn a_first_block_plus_ends_the_fence_with_the_item() {
    assert_eq!(to_html("> +\n- ``` js\n> q\n"), EMPTY_JS);
}

#[test]
fn an_ordered_or_nested_marker_and_a_tilde_fence_read_the_same() {
    assert_eq!(
        to_html("> +\n1. ``` js\n> q\n"),
        EMPTY_JS.replace("<ul>", "<ol>").replace("</ul>", "</ol>")
    );
    assert_eq!(to_html("> +\n- ~~~ js\n> q\n"), EMPTY_JS);
    assert_eq!(
        to_html("> +\n- - ``` js\n> q\n"),
        "<blockquote>\n  <ul>\n    <li>\n      <ul>\n        <li>\n          <pre><code class=\"language-js\"></code></pre>\n        </li>\n      </ul>\n    </li>\n  </ul>\n  <p>q</p>\n</blockquote>"
    );
}

#[test]
fn a_plus_on_its_own_line_after_a_quoted_paragraph() {
    assert_eq!(
        to_html("> a\n+\n- ``` js\n> q\n"),
        "<blockquote>\n  <p>a</p>\n  <ul>\n    <li>\n      <pre><code class=\"language-js\"></code></pre>\n    </li>\n  </ul>\n  <p>q</p>\n</blockquote>"
    );
}

#[test]
fn an_indented_follower_is_the_first_payload_line() {
    assert_eq!(
        to_html("> +\n- ``` js\n>   q\n"),
        "<blockquote>\n  <ul>\n    <li>\n      <pre><code class=\"language-js\">q\n</code></pre>\n    </li>\n  </ul>\n</blockquote>"
    );
}

#[test]
fn a_closed_fence_still_gets_its_separator() {
    assert_eq!(
        to_html("> +\n- ``` js\n  x\n  ```\n> q\n"),
        "<blockquote>\n  <ul>\n    <li>\n      <pre><code class=\"language-js\">x\n</code></pre>\n    </li>\n  </ul>\n  <p>q</p>\n</blockquote>"
    );
}
