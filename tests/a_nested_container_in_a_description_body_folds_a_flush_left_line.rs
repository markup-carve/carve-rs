//! A description body whose content is a nested LIST is still open to a
//! flush-left line: §24 C3 asks the fold of the innermost container the line
//! REACHES, and a list takes another item or another sibling block whatever
//! finished block its last item ended on (carve-rs#2313). Expectations are the
//! executable spec's, `renderDoc(parse(source))` at spec d3ba020.

use carve::to_html;

fn rows() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("ticket", ":: t\n: - ```\n    ```\n```\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <p><code></code></p>\n  </dd>\n</dl>"),
        ("paragraph_continues", ":: t\n: - ```\n    ```\n```\nx\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <p><code>\nx</code></p>\n  </dd>\n</dl>"),
        ("tilde", ":: t\n: - ~~~\n    ~~~\n~~~\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <p>~~~</p>\n  </dd>\n</dl>"),
        ("plain_text", ":: t\n: - ```\n    ```\nx\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <p>x</p>\n  </dd>\n</dl>"),
        ("nested_heading", ":: t\n: - # h\nx\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <h1 id=\"h\">h</h1>\n      </li>\n    </ul>\n    <p>x</p>\n  </dd>\n</dl>"),
        ("nested_table", ":: t\n: - | a |\nx\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <table>\n          <tbody>\n            <tr><td>a</td></tr>\n          </tbody>\n        </table>\n      </li>\n    </ul>\n    <p>x</p>\n  </dd>\n</dl>"),
        ("entry_below_survives", ":: t\n: - ```\n    ```\n```\n\n:: u\n: v\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <p><code></code></p>\n  </dd>\n  <dt>u</dt>\n  <dd>v</dd>\n</dl>"),
        ("ctrl_closer_ahead", ":: t\n: - ```\n    ```\n```\n```\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>\n<pre><code></code></pre>"),
        ("ctrl_nested_fence_open", ":: t\n: - ```\n```\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code>```\n</code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>"),
        ("ctrl_blank_ends_it", ":: t\n: - ```\n    ```\n\n```\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>\n<pre><code></code></pre>"),
        ("ctrl_heading_below", ":: t\n: - ```\n    ```\n# h\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>\n<section id=\"h\">\n  <h1>h</h1>\n</section>"),
        ("ctrl_marker_below", ":: t\n: - ```\n    ```\n- x\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n      <li>x</li>\n    </ul>\n  </dd>\n</dl>"),
        ("ctrl_direct_fence_body", ":: t\n: ```\n  ```\nx\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code></code></pre>\n  </dd>\n</dl>\n<p>x</p>"),
        ("ctrl_plain_body_lazy", ":: t\n: body\nlazy\n", "<dl>\n  <dt>t</dt>\n  <dd>body\nlazy</dd>\n</dl>"),
    ]
}

#[test]
fn a_nested_container_keeps_the_line_in_the_body() {
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
