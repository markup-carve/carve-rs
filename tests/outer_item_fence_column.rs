fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_continuation_marker_keeps_a_closed_fence_in_the_child_item() {
    let source = include_str!(
        "spec/tests/corpus/632-a-continuation-marker-uses-its-container-column-26.crv"
    );
    let expected = include_str!(
        "spec/tests/corpus/632-a-continuation-marker-uses-its-container-column-26.html"
    );
    assert_eq!(html(source), expected.trim_end());
}

#[test]
fn a_fence_at_the_outer_content_column_belongs_to_outer_item() {
    let source = "- outer\n  - inner\n  ```\n+\n  ```\nnext\n";
    let expected = "<ul>\n  <li>outer\n    <ul>\n      <li>inner</li>\n    </ul>\n    <pre><code></code></pre>\n  </li>\n</ul>\n<p>+\n<code>\nnext</code></p>";
    assert_eq!(html(source), expected);
}

#[test]
fn a_quoted_plus_does_not_move_the_outer_fence_into_the_inner_item() {
    let source = "- outer\n  - inner\n  ```\n> +\n  ```\nnext\n";
    let expected = "<ul>\n  <li>outer\n    <ul>\n      <li>inner</li>\n    </ul>\n    <pre><code></code></pre>\n  </li>\n</ul>\n<blockquote><p>+\n<code>\nnext</code></p></blockquote>";
    assert_eq!(html(source), expected);
}

#[test]
fn a_fence_at_the_inner_content_column_stays_in_the_inner_item() {
    let source = "- outer\n  - inner\n    ```\n+\n    ```\nnext\n";
    let expected = "<ul>\n  <li>outer\n    <ul>\n      <li>inner\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>+\n<code>\nnext</code></p>";
    assert_eq!(html(source), expected);
}

#[test]
fn an_unclosed_run_below_the_child_column_stays_in_its_paragraph() {
    assert_eq!(
        html("- outer\n  - inner\n  ```\nnext\n"),
        "<ul>\n  <li>outer\n    <ul>\n      <li>inner\n<code>\nnext</code></li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn repeated_outer_column_fences_keep_linear_owner_decisions() {
    let mut source = String::new();
    for _ in 0..128 {
        source.push_str("- outer\n  - inner\n  ```\n  ```\n\n");
    }
    assert_eq!(
        html(&source).matches("<pre><code></code></pre>").count(),
        128
    );
}
