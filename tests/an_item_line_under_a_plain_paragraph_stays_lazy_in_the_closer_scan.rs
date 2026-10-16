const PLAIN_COMMENT: &str = "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>para\n-\n<code>\ncode</code></p>\n  <div>\n    <div>\n      <p>after <strong>b</strong></p>\n    </div>\n  </div>\n</aside>";
const PLAIN_ATTR: &str = "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>para\n- {<span class=\"tag\"><strong>#x</strong></span>}\n<code>\ncode</code></p>\n  <div>\n    <div>\n      <p>after <strong>b</strong></p>\n    </div>\n  </div>\n</aside>";
const ORDERED_COMMENT: &str = "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>para\n2.\n<code>\ncode</code></p>\n  <div>\n    <div>\n      <p>after <strong>b</strong></p>\n    </div>\n  </div>\n</aside>";
const ITEM_CONTROL: &str = "<aside class=\"admonition note\" aria-label=\"Note\">\n  <ul>\n    <li>para</li>\n    <li></li>\n  </ul>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>";
const ORDERED_ITEM_CONTROL: &str = "<aside class=\"admonition note\" aria-label=\"Note\">\n  <ol>\n    <li>para</li>\n    <li></li>\n  </ol>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>";
const NESTED_ITEM_CONTROL: &str = "<aside class=\"admonition note\" aria-label=\"Note\">\n  <ul>\n    <li>para\n      <ul>\n        <li></li>\n      </ul>\n    </li>\n  </ul>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>";
const QUOTED_PLAIN_COMMENT: &str = "<aside class=\"admonition note\" aria-label=\"Note\">\n  <blockquote><p>para\n-\n<code>\ncode</code></p></blockquote>\n  <div>\n    <div>\n      <p>after <strong>b</strong></p>\n    </div>\n  </div>\n</aside>";
const QUOTED_ITEM_THEN_ITEM: &str = "<aside class=\"admonition note\" aria-label=\"Note\">\n  <blockquote>\n    <ul>\n      <li>para\n-\n<code>\ncode</code></li>\n    </ul>\n  </blockquote>\n  <div>\n    <div>\n      <p>after <strong>b</strong></p>\n    </div>\n  </div>\n</aside>";

#[test]
fn an_item_line_under_a_plain_paragraph_stays_lazy_in_the_closer_scan() {
    let cases = [
        ("para\n- %% c", PLAIN_COMMENT),
        ("para\n- {#x}", PLAIN_ATTR),
        ("para\n2. %% c", ORDERED_COMMENT),
        ("- para\n- %% c", ITEM_CONTROL),
        ("- para\n- {#x}", ITEM_CONTROL),
        ("1. para\n2. %% c", ORDERED_ITEM_CONTROL),
        ("- para\n  - %% c", NESTED_ITEM_CONTROL),
        ("> para\n> - %% c", QUOTED_PLAIN_COMMENT),
        ("> - para\n- %% c", QUOTED_ITEM_THEN_ITEM),
    ];
    for (body, expected) in cases {
        let source = format!("::: note\n{body}\n```\ncode\n::::\n:::\nafter *b*\n");
        assert_eq!(
            carve::to_html(&source).trim_end_matches('\n'),
            expected,
            "{source}"
        );
    }
}
