use carve::to_html;

#[test]
fn a_fence_run_folded_into_a_child_paragraph_leaves_the_outer_item_loose() {
    let source = "- outer\n  - head\n    ```\n    a\n   ```\n\n  tail\n";
    let expected = "<ul>\n  <li>outer\n    <ul>\n      <li>head\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>";
    assert_eq!(to_html(source).trim_end_matches('\n'), expected);
}

#[test]
fn a_fence_run_that_dedents_out_of_a_child_keeps_the_outer_item_tight() {
    let rows: &[(&str, &str, &str)] = &[
        (
            "1",
            r#"- outer
  - head

    ```
    a
  ```

  tail
"#,
            "<ul>\n  <li>outer\n    <ul>\n      <li><p>head</p>\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>",
        ),
        (
            "2",
            r#"- outer
  - head

    ```
    a
   ```

  tail
"#,
            "<ul>\n  <li>outer\n    <ul>\n      <li><p>head</p>\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>",
        ),
        (
            "3",
            r#"- outer
  - head

    ~~~
    a
   ~~~

  tail
"#,
            "<ul>\n  <li>outer\n    <ul>\n      <li><p>head</p>\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>",
        ),
        (
            "4",
            r#"- outer
  - head

        ```
        a
   ```

  tail
"#,
            "<ul>\n  <li>outer\n    <ul>\n      <li><p>head</p>\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>",
        ),
        (
            "5",
            r#"- outer
  - head

    ```
    a
   ```

    tail
"#,
            "<ul>\n  <li>outer\n    <ul>\n      <li><p>head</p>\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\n tail\n</code></pre>\n  </li>\n</ul>",
        ),
        (
            "6",
            r#"- outer
  - one
  - two

    ```
    a
  ```

  tail
"#,
            "<ul>\n  <li>outer\n    <ul>\n      <li><p>one</p></li>\n      <li><p>two</p>\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>",
        ),
        (
            "7",
            r#"1. outer
   - head

     ```
     a
   ```

   tail
"#,
            "<ol>\n  <li>outer\n    <ul>\n      <li><p>head</p>\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ol>",
        ),
        (
            "8",
            r#"> - outer
>   - head
>
>     ```
>     a
>   ```
>
>   tail
"#,
            "<blockquote>\n  <ul>\n    <li>outer\n      <ul>\n        <li><p>head</p>\n          <pre><code>a\n</code></pre>\n        </li>\n      </ul>\n      <pre><code>\ntail\n</code></pre>\n    </li>\n  </ul>\n</blockquote>",
        ),
        (
            "9",
            r#"- outer
  - head

    ```
    a
  ```

  tail
- next
"#,
            "<ul>\n  <li>outer\n    <ul>\n      <li><p>head</p>\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n  <li>next</li>\n</ul>",
        ),
        (
            "10",
            r#"- outer
  - head

    ```
    a
  ```

- next
"#,
            "<ul>\n  <li><p>outer</p>\n    <ul>\n      <li><p>head</p>\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\n</code></pre>\n  </li>\n  <li><p>next</p></li>\n</ul>",
        ),
        (
            "11",
            r#"- outer
  - head

    ```
    a
    ```

  tail
"#,
            "<ul>\n  <li><p>outer</p>\n    <ul>\n      <li><p>head</p>\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <p>tail</p>\n  </li>\n</ul>",
        ),
        (
            "12",
            r#"- outer
  - head

    ```
    a
    ```
  ```
  b

  c
"#,
            "<ul>\n  <li>outer\n    <ul>\n      <li><p>head</p>\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>b\n\nc\n</code></pre>\n  </li>\n</ul>",
        ),
        (
            "13",
            r#"- outer
  - head

    ~~~
    a
  ```

  tail
"#,
            "<ul>\n  <li>outer\n    <ul>\n      <li><p>head</p>\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>",
        ),
        (
            "14",
            r#"- outer
  - head
    - deep

      ```
      a
  ```

  tail
"#,
            "<ul>\n  <li>outer\n    <ul>\n      <li>head\n        <ul>\n          <li><p>deep</p>\n            <pre><code>a\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>",
        ),
    ];
    let failures: Vec<_> = rows
        .iter()
        .filter_map(|(name, source, expected)| {
            let actual = to_html(source);
            (actual.trim_end_matches('\n') != *expected).then(|| format!("row {name}: {actual:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
// Exact HTML from the fixed carve-js reference for the folded fence runs and controls.
#[test]
fn a_folded_child_fence_does_not_hide_the_outer_second_paragraph() {
    let cases = [
        ("- outer\n  - head\n     ```\n     a\n  ```\n\n  tail\n", "<ul>\n  <li>outer\n    <ul>\n      <li>head\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>"),
        ("- outer\n  - head\n    more\n    ```\n    a\n   ```\n\n  tail\n", "<ul>\n  <li>outer\n    <ul>\n      <li>head\nmore\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>"),
        ("- outer\n  - head\n    ```\n    a\n   ```\n\n  tail\n", "<ul>\n  <li>outer\n    <ul>\n      <li>head\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>"),
        ("- outer\n  - head\n    ~~~\n    a\n   ~~~\n\n  tail\n", "<ul>\n  <li>outer\n    <ul>\n      <li>head\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>"),
        ("1. outer\n   - head\n     ```\n     a\n    ```\n\n   tail\n", "<ol>\n  <li>outer\n    <ul>\n      <li>head\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ol>"),
        ("> - outer\n>   - head\n>     ```\n>     a\n>    ```\n>\n>   tail\n", "<blockquote>\n  <ul>\n    <li>outer\n      <ul>\n        <li>head\n          <pre><code>a\n</code></pre>\n        </li>\n      </ul>\n      <pre><code>\ntail\n</code></pre>\n    </li>\n  </ul>\n</blockquote>"),
        ("- outer\n  - head\n     ```\n     a\n   ```\n\n  tail\n", "<ul>\n  <li>outer\n    <ul>\n      <li>head\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>"),
        ("- outer\n  - head\n\n    ```\n    a\n   ```\n\n  tail\n", "<ul>\n  <li>outer\n    <ul>\n      <li><p>head</p>\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>"),
        ("- outer\n  - head\n\n    ```\n    a\n  ```\n\n  tail\n", "<ul>\n  <li>outer\n    <ul>\n      <li><p>head</p>\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code>\ntail\n</code></pre>\n  </li>\n</ul>"),
        ("- outer\n  - head\n    ```\n    a\n    ```\n\n  tail\n", "<ul>\n  <li><p>outer</p>\n    <ul>\n      <li>head\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <p>tail</p>\n  </li>\n</ul>"),
        ("- outer\n  - head\n    ```\n    a\n   ```\n- next\n", "<ul>\n  <li>outer\n    <ul>\n      <li>head\n        <pre><code>a\n</code></pre>\n      </li>\n    </ul>\n    <pre><code></code></pre>\n  </li>\n  <li>next</li>\n</ul>"),
    ];
    for (source, expected) in cases {
        assert_eq!(carve::to_html(source), expected, "{source:?}");
    }
}
