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
    // corpus 597-5, after a comment line
    ("::: note\n%% c\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a comment-only quote
    ("::: note\n> %% c\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <blockquote>\n\n  </blockquote>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a comment-only item
    ("::: note\n- %% c\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <ul>\n    <li></li>\n  </ul>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a comment-only bare-dot item
    ("::: note\n. %% c\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <ol>\n    <li></li>\n  </ol>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a comment-only item with attributes
    ("::: note\n3.{.c} %% c\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <ol start=\"3\">\n    <li class=\"c\"></li>\n  </ol>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // an open quote paragraph keeps the fence as text
    ("::: note\n> quote\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <blockquote><p>quote\n<code>\ncode</code></p></blockquote>\n  <div>\n    <div>\n      <p>after <strong>b</strong></p>\n    </div>\n  </div>\n</aside>"),
    // an open item paragraph keeps the fence as text
    ("::: note\n- item\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <ul>\n    <li>item\n<code>\ncode</code></li>\n  </ul>\n  <div>\n    <div>\n      <p>after <strong>b</strong></p>\n    </div>\n  </div>\n</aside>"),
    // corpus 606, after an attribute line
    ("::: note\n{#x}\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <pre id=\"x\"><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // corpus 606-2, after an attribute line, nested
    ("::: outer\n:::: inner\n{.c}\n```\ncode\n:::\n::::\n:::\nafter *b*\n", "<div class=\"outer\">\n  <div class=\"inner\">\n    <pre class=\"c\"><code>code\n:::\n</code></pre>\n  </div>\n</div>\n<p>after <strong>b</strong></p>"),
    // corpus 606-3, after an attribute-only quote
    ("::: note\n> {#x}\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <blockquote>\n\n  </blockquote>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // corpus 606-4, after an attribute-only item
    ("::: note\n- {#x}\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <ul>\n    <li></li>\n  </ul>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a table row
    ("::: note\npara\n| t |\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>para</p>\n  <table>\n    <tbody>\n      <tr><td>t</td></tr>\n    </tbody>\n  </table>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a table row with a row attribute
    ("::: note\npara\n| t |{.x}\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>para</p>\n  <table>\n    <tbody>\n      <tr class=\"x\"><td>t</td></tr>\n    </tbody>\n  </table>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a table with a separator row
    ("::: note\npara\n| a | b |\n|---|---|\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>para</p>\n  <table>\n    <thead>\n      <tr><th scope=\"col\">a</th><th scope=\"col\">b</th></tr>\n    </thead>\n  </table>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a quoted table row
    ("::: note\n> para\n> | t |\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <blockquote>\n    <p>para</p>\n    <table>\n      <tbody>\n        <tr><td>t</td></tr>\n      </tbody>\n    </table>\n  </blockquote>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a nested quoted table row
    ("::: note\n> > para\n> > | t |\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <blockquote>\n    <blockquote>\n      <p>para</p>\n      <table>\n        <tbody>\n          <tr><td>t</td></tr>\n        </tbody>\n      </table>\n    </blockquote>\n  </blockquote>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a table-only item
    ("::: note\n- para\n- | t |\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <ul>\n    <li>para</li>\n    <li>\n      <table>\n        <tbody>\n          <tr><td>t</td></tr>\n        </tbody>\n      </table>\n    </li>\n  </ul>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a table-only ordered item
    ("::: note\n1. para\n2. | t |\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <ol>\n    <li>para</li>\n    <li>\n      <table>\n        <tbody>\n          <tr><td>t</td></tr>\n        </tbody>\n      </table>\n    </li>\n  </ol>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a link definition
    ("::: note\npara\n[r]: /u\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>para</p>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a quoted link definition
    ("::: note\n> para\n> [r]: /u\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <blockquote><p>para</p></blockquote>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a quoted titled link definition
    ("::: note\n> para\n> [r]: /u \"t\"\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <blockquote><p>para</p></blockquote>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a quoted footnote definition
    ("::: note\n> para\n> [^r]: note\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <blockquote><p>para</p></blockquote>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a nested quoted link definition
    ("::: note\n> > para\n> > [r]: /u\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <blockquote>\n    <blockquote><p>para</p></blockquote>\n  </blockquote>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // after a definition-only item
    ("::: note\n- para\n- [r]: /u\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <ul>\n    <li>para</li>\n    <li></li>\n  </ul>\n  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>"),
    // a quoted pipe line without a closing pipe keeps the paragraph open
    ("::: note\n> para\n> | t\n```\ncode\n::::\n:::\nafter *b*\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <blockquote><p>para\n| t\n<code>\ncode</code></p></blockquote>\n  <div>\n    <div>\n      <p>after <strong>b</strong></p>\n    </div>\n  </div>\n</aside>"),
    // an indented run is no fence
    ("::: note\n  ```\n  code\n  ::::\n:::\nafter\n", "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p><code>\ncode\n::::</code></p>\n</aside>\n<p>after</p>"),
];

#[test]
#[ignore = "rule A ledger: carve-rs#3525"]
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
