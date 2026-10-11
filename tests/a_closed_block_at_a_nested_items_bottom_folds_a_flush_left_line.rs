//! Closed marker-line fences follow the deepest-frame ruling in carve#2895.

use carve::to_html;

fn rows() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("ticket", "- - ```\n    ```\nx\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"),
        ("fence_line_bare", "- - ```\n    ```\n```\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<pre><code></code></pre>"),
        ("fence_line_lang", "- - ```\n    ```\n```rust\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<pre><code class=\"language-rust\"></code></pre>"),
        ("tilde_line_lang", "- - ```\n    ```\n~~~rust\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<pre><code class=\"language-rust\"></code></pre>"),
        ("raw_block", "- - ```=html\n    ```\nx\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        \n      </li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"),
        ("raw_block_lang", "- - ```=html\n    ```\n```rust\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        \n      </li>\n    </ul>\n  </li>\n</ul>\n<pre><code class=\"language-rust\"></code></pre>"),
        ("three_levels", "- - - ```\n      ```\nx\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <pre><code></code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"),
        ("fence_line_then_text", "- - ```\n    ```\n```rust\nx\n", "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<pre><code class=\"language-rust\">x\n</code></pre>"),
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
        ("body_fence_lang", ":: t\n: - ```\n    ```\n```rust\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>\n<pre><code class=\"language-rust\"></code></pre>"),
        ("body_tilde_lang", ":: t\n: - ```\n    ```\n~~~rust\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>\n<pre><code class=\"language-rust\"></code></pre>"),
    ]
}

#[test]
fn closed_nested_blocks_follow_the_deepest_frame_ruling() {
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
