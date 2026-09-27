use carve::{to_carve, to_html, to_json_with_options, Options};

fn inspect(node: &serde_json::Value, source_len: usize, content: &mut Vec<String>) {
    match node {
        serde_json::Value::Object(map) => {
            if map.get("type").and_then(|value| value.as_str()) == Some("code_block") {
                content.push(map["content"].as_str().unwrap().to_string());
            }
            if let Some(end) = map.get("endOffset").and_then(|value| value.as_u64()) {
                assert!(end <= source_len as u64, "{node}");
            }
            for value in map.values() {
                inspect(value, source_len, content);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                inspect(item, source_len, content);
            }
        }
        _ => {}
    }
}

#[test]
fn whitespace_only_verbatim_lines_keep_the_residue_past_the_fence_column() {
    for (source, expected) in [
        ("```\na\n  \nb\n```\n", "a\n  \nb"),
        ("- item\n\n  ```\n  a\n  \n  b\n  ```\n", "a\n\nb"),
        ("- item\n\n    ```\n    a\n    \n    b\n    ```\n", "a\n\nb"),
        (
            "- item\n\n    ```\n    a\n      \n    b\n    ```\n",
            "a\n  \nb",
        ),
        (
            "x[^1]\n\n[^1]: note\n\n    ```\n    a\n      \n    b\n    ```\n",
            "a\n  \nb",
        ),
        (
            "x[^1]\n\n[^1]: note\n\n      ```\n      a\n      \n      b\n      ```\n",
            "a\n\nb",
        ),
        (
            "x[^1]\n\n[^1]: note\n\n      ```\n      a\n        \n      b\n      ```\n",
            "a\n  \nb",
        ),
        (
            "- - item\n\n    ```\n    a\n      \n    b\n    ```\n",
            "a\n  \nb",
        ),
        (
            "- - item\n\n      ```\n      a\n      \n      b\n      ```\n",
            "a\n\nb",
        ),
        (
            "- - item\n\n      ```\n      a\n        \n      b\n      ```\n",
            "a\n  \nb",
        ),
        (
            ":: t\n:  d\n\n     ```\n     a\n       \n     b\n     ```\n",
            "a\n  \nb",
        ),
        (
            "> - item\n>\n>     ```\n>     a\n>       \n>     b\n>     ```\n",
            "a\n  \nb",
        ),
        (
            "- item\n\n    ```\n    a\n     \n    b\n    ```\n",
            "a\n \nb",
        ),
        ("- item\n\n   ```\n   a\n\t\n   b\n   ```\n", "a\n \nb"),
        ("- item\n\n    ```\n    a\n  \n    b\n    ```\n", "a\n\nb"),
        ("- ```\n  a\n\t", "a\n  "),
        (":: t\n:  d\n\n   ```\n   a\n\t\n   b\n   ```\n", "a\n \nb"),
    ] {
        for input in [source.to_string(), to_carve(source)] {
            for options in [Options::default(), Options::default().with_positions(true)] {
                let json = to_json_with_options(&input, &options);
                let mut content = Vec::new();
                inspect(
                    &serde_json::from_str(&json).unwrap(),
                    input.len(),
                    &mut content,
                );
                assert!(!content.is_empty(), "{input:?}");
                assert!(
                    content.iter().all(|code| code == expected),
                    "{input:?}: {content:?}"
                );
            }
            assert!(
                to_html(&input).contains(&format!("<pre><code>{expected}\n</code></pre>")),
                "{input:?}"
            );
        }
    }
    for (source, plain) in [
        ("- a\n    \n  b\n", "- a\n\n  b\n"),
        ("- a\n    \n \n\t\n- b\n", "- a\n\n\n\n- b\n"),
    ] {
        assert_eq!(to_html(source), to_html(plain));
        assert_eq!(to_carve(source), to_carve(plain));
    }
}
