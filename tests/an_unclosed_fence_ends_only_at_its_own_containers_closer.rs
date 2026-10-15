//! Inside an unclosed code fence only the innermost container's own closer
//! is structural; any other colon line, of any width, is fence content
//! (A CLOSER IS REQUIRED, markup-carve/carve#3201). Expectations match
//! carve-js (markup-carve/carve-js#3116).

const CASES: &[(&str, &str)] = &[
    // corpus 597, nested
    ("::: outer\n:::: inner\n```\ncode\n:::\n::::\n:::\nafter *b*\n", "<div class=\"outer\">\n  <div class=\"inner\">\n    <pre><code>code\n:::\n</code></pre>\n  </div>\n</div>\n<p>after <strong>b</strong></p>"),
    // corpus 597-2, swapped widths
    (":::: outer\n::: inner\n```\ncode\n::::\n:::\n::::\nafter *b*\n", "<div class=\"outer\">\n  <div class=\"inner\">\n    <pre><code>code\n::::\n</code></pre>\n  </div>\n</div>\n<p>after <strong>b</strong></p>"),
    // corpus 597-3, single container
    ("::: note\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // corpus 597-4, closed fence control
    ("::: outer\n:::: inner\n```\ncode\n:::\n```\n::::\n:::\nafter *b*\n", "<div class=\"outer\">\n  <div class=\"inner\">\n    <pre><code>code\n:::\n</code></pre>\n  </div>\n</div>\n<p>after <strong>b</strong></p>"),
    // after a closed list
    ("::: note\n- item\n\n```\ncode\n::::\n:::\nafter\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <ul>\n    <li>item</li>\n  </ul>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after</p>"),
    // a typed opener inside the fence
    ("::: note\n```\ncode\n::: warning\n:::\nafter\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <pre><code>code\n::: warning\n</code></pre>\n</aside>\n<p>after</p>"),
    // in a block quote
    ("> ::: note\n> ```\n> code\n> ::::\n> :::\n\nafter\n", "<blockquote>\n  <aside class=\"admonition note\" aria-label=\"Note\">\n    <pre><code>code\n::::\n</code></pre>\n  </aside>\n</blockquote>\n<p>after</p>"),
    // in a list item
    ("- ::: note\n  ```\n  code\n  ::::\n  :::\n\nafter\n", "<ul>\n  <li>\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <pre><code>code\n::::\n</code></pre>\n    </aside>\n  </li>\n</ul>\n<p>after</p>"),
    // a tilde fence
    ("::: note\n~~~\ncode\n::::\n:::\nafter\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after</p>"),
    // a raw fence
    ("::: note\n``` =html\n<b>\n::::\n:::\nafter\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <b>\n::::\n</aside>\n<p>after</p>"),
    // an open paragraph keeps the fence as text
    ("::: note\ntext\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>text\n<code>\ncode</code></p>\n  <div>\n    <div>\n      <p>after <strong>b</strong></p>\n    </div>\n  </div>\n</aside>"),
    // an indented run is no fence
    ("::: note\n  ```\n  code\n  ::::\n:::\nafter\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p><code>\ncode\n::::</code></p>\n</aside>\n<p>after</p>"),
];

#[test]
fn an_unclosed_fence_ends_only_at_its_own_containers_closer() {
    let mut failures = Vec::new();
    for (source, expected) in CASES {
        let got = carve::to_html(source);
        if got.trim_end_matches('\n') != *expected {
            failures.push(format!(
                "source:\n{source}\nexpected:\n{expected}\ngot:\n{got}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}
