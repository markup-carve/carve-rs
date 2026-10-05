//! A CLOSED FENCE OR RAW BLOCK at a nested item's bottom leaves the OUTER item
//! open to a flush-left non-opener; a heading, a table or a comment in the same
//! position leaves the list. markup-carve/carve#2734 rules that split normative
//! rather than a uniform rule, because a uniform column-0 fold was measured to
//! move three rows that already agreed with the spec. The info string on a
//! folded fence line changes nothing (markup-carve/carve#2735).
//!
//! The two arms are each other's control: a change that folded every closed
//! block would pass every `fence`/`raw` row here and fail `ctrl_heading`,
//! `ctrl_table` and `ctrl_comment`. `ctrl_fence_not_on_the_lead` is the other
//! side: a fence the item closed BELOW its lead ends the list, so the arm turns
//! on the lead and not on any closed fence. Expectations are the executable spec's,
//! `renderDoc(parse(source))` at spec 8b68a46 (carve-rs#2313).

use carve::to_html;

fn rows() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("ticket", "- - ```\n    ```\nx\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    x\n  </li>\n</ul>"),
        ("fence_line_bare", "- - ```\n    ```\n```\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <code></code>\n  </li>\n</ul>"),
        ("fence_line_lang", "- - ```\n    ```\n```rust\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <code>rust</code>\n  </li>\n</ul>"),
        ("tilde_line_lang", "- - ```\n    ```\n~~~rust\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    ~~~rust\n  </li>\n</ul>"),
        ("raw_block", "- - ```=html\n    ```\nx\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        \n      </li>\n    </ul>\n    x\n  </li>\n</ul>"),
        ("raw_block_lang", "- - ```=html\n    ```\n```rust\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        \n      </li>\n    </ul>\n    <code>rust</code>\n  </li>\n</ul>"),
        ("three_levels", "- - - ```\n      ```\nx\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <pre><code></code></pre>\n          </li>\n        </ul>\n        x\n      </li>\n    </ul>\n  </li>\n</ul>"),
        ("fence_line_then_text", "- - ```\n    ```\n```rust\nx\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <code>rust\nx</code>\n  </li>\n</ul>"),
        ("ctrl_heading", "- - # h\nx\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <h1 id=\"h\">h</h1>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"),
        ("ctrl_table", "- - | a |\nx\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <table>\n          <tbody>\n            <tr><td>a</td></tr>\n          </tbody>\n        </table>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"),
        ("ctrl_comment", "- - %% c\nx\n", "<ul>\n  <li>\n    <ul>\n      <li></li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"),
        ("ctrl_closer_ahead", "- - ```\n    ```\n```\n```\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<pre><code></code></pre>"),
        ("ctrl_blank_ends_it", "- - ```\n    ```\n\nx\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"),
        ("ctrl_marker_below", "- - ```\n    ```\n- y\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n  <li>y</li>\n</ul>"),
        ("ctrl_single_level", "- ```\n  ```\nx\n", "<ul>\n  <li>\n    <pre><code></code></pre>\n  </li>\n</ul>\n<p>x</p>"),
        ("ctrl_document", "```\n```\nx\n", "<pre><code></code></pre>\n<p>x</p>"),
        ("ctrl_open_paragraph", "- - p\nx\n", "<ul>\n  <li>\n    <ul>\n      <li>p\nx</li>\n    </ul>\n  </li>\n</ul>"),
        ("ctrl_quote", "- - > q\nx\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <blockquote><p>q\nx</p></blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>"),
        ("ctrl_fence_not_on_the_lead", "- - b\n  %% c\n    ```\ntail\n    ```\n", "<ul>\n  <li>\n    <ul>\n      <li>b\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>tail\n<code></code></p>"),
        ("ctrl_thematic", "- - ---\nx\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <hr>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"),
        ("body_fence_lang", ":: t\n: - ```\n    ```\n```rust\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <p><code>rust</code></p>\n  </dd>\n</dl>"),
        ("body_tilde_lang", ":: t\n: - ```\n    ```\n~~~rust\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <p>~~~rust</p>\n  </dd>\n</dl>"),
    ]
}

#[test]
fn a_closed_verbatim_block_keeps_the_line_in_the_outer_item() {
    let mut failures = Vec::new();
    for (name, src, expected) in rows() {
        let got = to_html(src);
        if got.trim_end() != expected {
            failures.push(format!(
                "{name}\n--- expected\n{expected}\n--- got\n{}\n",
                got.trim_end()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
