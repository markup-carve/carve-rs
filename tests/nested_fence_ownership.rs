// Category 509 goldens from markup-carve/carve@40d5922a.
// The extra fixtures cover marker widths, payload, siblings, and tabs.
use carve::Options;

#[test]
fn nested_fence_ownership_matches_spec_509() {
    let rows = [
        ("509-a-fence-closer-below-a-nested-item-s-column-ends-containers-down-to-its-owner-10.crv", "- a\n  - b\n    ```\n    p\nx\n    ```\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code>p\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>x\n<code></code></p>\n"),
        ("509-a-fence-closer-below-a-nested-item-s-column-ends-containers-down-to-its-owner-11.crv", "- a\n  - b\n    - c\n      ```\n      p\nx\n      ```\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <ul>\n          <li>c\n            <pre><code>p\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>x\n<code></code></p>\n"),
        ("509-a-fence-closer-below-a-nested-item-s-column-ends-containers-down-to-its-owner-12.crv", "- a\n  - b\n    ```\n    p\n  - c\n    ```\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n<code>\np</code></li>\n      <li>c\n<code></code></li>\n    </ul>\n  </li>\n</ul>\n"),
        ("509-a-fence-closer-below-a-nested-item-s-column-ends-containers-down-to-its-owner-2.crv", "- a\n  - b\n\n    ```\n    p\n ```\n\n    tail\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code>p\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p><code></code></p>\n<p>tail</p>\n"),
        ("509-a-fence-closer-below-a-nested-item-s-column-ends-containers-down-to-its-owner-3.crv", "- a\n  - b\n\n    ```\n    p\n  ```\n\n    tail\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code>p\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\n  tail\n</code></pre>\n  </li>\n</ul>\n"),
        ("509-a-fence-closer-below-a-nested-item-s-column-ends-containers-down-to-its-owner-4.crv", "- a\n  - b\n    ```\n    p\n```\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n<code>\np\n</code></li>\n    </ul>\n  </li>\n</ul>\n"),
        ("509-a-fence-closer-below-a-nested-item-s-column-ends-containers-down-to-its-owner-5.crv", "- a\n  - b\n    ```\n    p\n ```\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n<code>\np\n</code></li>\n    </ul>\n  </li>\n</ul>\n"),
        ("509-a-fence-closer-below-a-nested-item-s-column-ends-containers-down-to-its-owner-6.crv", "- a\n  - b\n    - c\n\n      ```\n      p\n```\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <ul>\n          <li>c\n            <pre><code>p\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>\n<pre><code></code></pre>\n"),
        ("509-a-fence-closer-below-a-nested-item-s-column-ends-containers-down-to-its-owner-7.crv", "- a\n  - b\n    - c\n\n      ```\n      p\n  ```\n\n      tail\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <ul>\n          <li>c\n            <pre><code>p\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n    <pre><code>\n    tail\n</code></pre>\n  </li>\n</ul>\n"),
        ("509-a-fence-closer-below-a-nested-item-s-column-ends-containers-down-to-its-owner-8.crv", "- a\n  - b\n\n    ~~~\n    p\n ~~~\n\n    tail\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code>p\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>~~~</p>\n<p>tail</p>\n"),
        ("509-a-fence-closer-below-a-nested-item-s-column-ends-containers-down-to-its-owner-9.crv", "- a\n  - b\n\n    ```=html\n    <b>p</b>\n```\n\n    tail\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <b>p</b>\n      </li>\n    </ul>\n  </li>\n</ul>\n<pre><code>\n    tail\n</code></pre>\n"),
        ("509-a-fence-closer-below-a-nested-item-s-column-ends-containers-down-to-its-owner.crv", "- a\n  - b\n\n    ```\n    p\n```\n\n    tail\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code>p\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<pre><code>\n    tail\n</code></pre>\n"),
    ];
    let mut failures = Vec::new();
    for (name, source, expected) in rows {
        for positions in [false, true] {
            let options = Options {
                positions,
                ..Options::default()
            };
            let actual = carve::to_html_with_options(source, &options);
            if actual.trim_end() != expected.trim_end() {
                failures.push(format!("{name} positions={positions}\n{actual}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn nested_fence_markers_payload_and_siblings() {
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/nested-fence-ownership.json")).unwrap();
    assert_eq!(rows.len(), 17);
    for row in rows {
        for positions in [false, true] {
            let options = Options {
                positions,
                ..Options::default()
            };
            assert_eq!(
                carve::to_html_with_options(row["source"].as_str().unwrap(), &options).trim_end(),
                row["html"].as_str().unwrap().trim_end(),
                "{} positions={positions}",
                row["name"]
            );
        }
    }
}

#[test]
fn interrupted_fence_keeps_its_source_span() {
    let source = "- a\n  - b\n    ```\n    p\nx\n    ```\n";
    let options = Options {
        positions: true,
        ..Options::default()
    };
    let doc: serde_json::Value =
        serde_json::from_str(&carve::to_json_with_options(source, &options)).unwrap();
    let code = &doc["children"][0]["items"][0]["children"][1]["items"][0]["children"][1];
    assert_eq!(code["type"], "code_block");
    assert_eq!(code["content"], "p\n");
    assert_eq!(
        code["pos"],
        serde_json::json!({
            "startLine": 3, "endLine": 4, "startColumn": 5, "endColumn": 6,
            "startOffset": 14, "endOffset": 23
        })
    );
    assert_eq!(doc["children"][1]["pos"]["startLine"], 5);
}
