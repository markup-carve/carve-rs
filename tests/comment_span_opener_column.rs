use carve::{parse_with_options, render_html, to_html, Options};

fn assert_html(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected, "source: {source:?}");
    let source_attribute = regex::Regex::new(r#" data-source-line="[0-9]+""#).unwrap();
    for (positions, source_lines) in [(true, false), (false, true), (true, true)] {
        let options = Options {
            positions,
            source_lines,
            ..Options::default()
        };
        let html = render_html(&parse_with_options(source, &options)).unwrap();
        assert_eq!(
            source_attribute.replace_all(&html, ""),
            expected,
            "positions={positions}, source_lines={source_lines}, source: {source:?}"
        );
    }
}

#[test]
fn a_comment_closer_uses_the_openers_column_for_following_text() {
    for (marker, column, tag, attrs) in [
        ("- ", 2, "ul", ""),
        ("1. ", 3, "ol", ""),
        ("10. ", 4, "ol", " start=\"10\""),
    ] {
        for width in [3, 5] {
            let run = "%".repeat(width);
            for closer in 0..=8 {
                for opener in [1, column] {
                    let source = format!(
                        "{marker}item\n{}{run}\n{}hidden\n{}{run}\ntail\n",
                        " ".repeat(opener),
                        " ".repeat(opener),
                        " ".repeat(closer)
                    );
                    let expected =
                        format!("<{tag}{attrs}>\n  <li>item</li>\n</{tag}>\n<p>tail</p>");
                    assert_html(&source, &expected);
                }
            }
        }
    }
}

#[test]
fn a_marker_line_comment_keeps_its_closer_and_leaves_no_paragraph() {
    for closer in 0..=8 {
        for follower in ["tail", " tail"] {
            assert_html(
                &format!("- %%%\n  hidden\n{}%%%\n{follower}\n", " ".repeat(closer)),
                "<ul>\n  <li></li>\n</ul>\n<p>tail</p>",
            );
        }
    }
}

#[test]
fn folded_code_keeps_block_markers_as_payload() {
    for payload in ["- not item", "1. not item", "%%%", "::: note", "# heading"] {
        assert_html(
            &format!("- item\n %%%\n hidden\n  %%%\n  ```\n  {payload}\n"),
            &format!("<ul>\n  <li>item</li>\n</ul>\n<pre><code>{payload}\n</code></pre>"),
        );
    }
}

#[test]
fn a_code_closer_outside_the_item_does_not_change_interruption() {
    assert_html(
        "- item\n %%%\n hidden\n  %%%\n  ```\ntail\n\n%%%\n```\n%%%\n",
        "<ul>\n  <li>item</li>\n</ul>\n<pre><code>tail\n\n%%%\n</code></pre>",
    );
    assert_html(
        "- item\n %%%\n hidden\n  %%%\n  ```\ntail\n\n- other\n\n      ```\n",
        "<ul>\n  <li>item</li>\n</ul>\n<pre><code>tail\n\n- other\n\n    ```\n</code></pre>",
    );
}

#[test]
fn a_below_column_line_comment_keeps_the_same_code_fold() {
    assert_html(
        "- item\n %% c\n  ```\ntail\n",
        "<ul>\n  <li>item</li>\n</ul>\n<pre><code>tail\n</code></pre>",
    );
}

#[test]
fn folded_code_payload_still_controls_later_lazy_continuation() {
    for payload in ["# h", "::: note", "%% c", "x\n  # h"] {
        let code = payload.replace("\n  ", "\n");
        assert_html(
            &format!("- item\n %%%\n hidden\n  %%%\n  ```\n  {payload}\ntail\n"),
            &format!("<ul>\n  <li>item</li>\n</ul>\n<pre><code>{code}\ntail\n</code></pre>"),
        );
    }
    for (payload, escaped) in [("- x", "- x"), ("> q", "&gt; q")] {
        assert_html(
            &format!("- item\n %% c\n  ```\n  {payload}\ntail\n"),
            &format!("<ul>\n  <li>item</li>\n</ul>\n<pre><code>{escaped}\ntail\n</code></pre>"),
        );
    }
}

#[test]
fn a_lazy_backtick_line_is_code_payload() {
    assert_html(
        "- item\n %%%\n hidden\n  %%%\n  ```\ntail\n```\nafter\n",
        "<ul>\n  <li>item</li>\n</ul>\n<pre><code>tail\n</code></pre>\n<p>after</p>",
    );
}

#[test]
fn nested_comment_code_folds_use_the_authored_fence_column() {
    // Expected HTML comes from the spec renderer at 56b21217.
    for (source, expected) in [
        ("- a\n  - b\n %% c\n    ```\ntail\n", "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<pre><code>tail\n</code></pre>"),
        ("- a\n  - b\n    - c\n %% c\n      ```\ntail\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <ul>\n          <li>c</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>\n<pre><code>tail\n</code></pre>"),
        ("- - b\n %% c\n    ```\ntail\n", "<ul>\n  <li>\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<pre><code>tail\n</code></pre>"),
        ("1. a\n   10. b\n %% c\n       ```\ntail\n", "<ol>\n  <li>a\n    <ol start=\"10\">\n      <li>b</li>\n    </ol>\n  </li>\n</ol>\n<pre><code>tail\n</code></pre>"),
    ] {
        assert_html(source, expected);
    }
}

#[test]
fn a_closed_nested_fence_still_ends_at_the_dedent() {
    for column in [0, 1] {
        for fence in ["```", "~~~"] {
            assert_html(
                &format!("- a\n  - b\n{}%% c\n    {fence}\ntail\n    {fence}\n", " ".repeat(column)),
                "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<pre><code>tail\n</code></pre>",
            );
        }
    }
}

#[test]
fn a_blank_ends_the_nested_lazy_code_fold() {
    assert_html(
        "- a\n  - b\n %% c\n    ```\ntail\n\nend\n",
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<pre><code>tail\n\nend\n</code></pre>",
    );
}

#[test]
fn a_fence_below_the_nested_column_is_lazy_code_payload() {
    assert_html(
        "- a\n  - b\n %% c\n    ```\ntail\n  ```\nafter\n",
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<pre><code>tail\n</code></pre>\n<p>after</p>",
    );
}

#[test]
fn a_comment_at_the_outer_column_keeps_the_closed_fence_in_the_inner_item() {
    assert_html(
        "- - b\n  %% c\n    ```\ntail\n    ```\n",
        "<ul>\n  <li>\n    <ul>\n      <li>b</li>\n    </ul>\n    <pre><code></code></pre>\n  </li>\n</ul>\n<p>tail</p>\n<pre><code></code></pre>",
    );
}

#[test]
fn blocks_at_an_ancestor_column_end_the_nested_code_fold() {
    for (suffix, expected) in [
        ("  - x\n", "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<pre><code>tail\n- x\n</code></pre>"),
        ("  > q\n", "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<pre><code>tail\n&gt; q\n</code></pre>"),
        ("  # h\n", "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<pre><code>tail\n# h\n</code></pre>"),
    ] {
        assert_html(&format!("- a\n  - b\n %% c\n    ```\ntail\n{suffix}"), expected);
    }
}

#[test]
fn lazy_code_between_item_columns_drops_its_placing_indent() {
    for payload in ["x", "- x"] {
        assert_html(
            &format!("- a\n  - b\n %% c\n    ```\ntail\n   {payload}\n"),
            &format!("<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<pre><code>tail\n{payload}\n</code></pre>"),
        );
    }
}

#[test]
fn the_nested_code_fold_survives_explicit_quote_prefixes() {
    assert_html(
        "> - a\n>   - b\n> %% c\n>     ```\n> tail\n",
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>b</li>\n      </ul>\n    </li>\n  </ul>\n  <pre><code>tail\n</code></pre>\n</blockquote>",
    );
}

#[test]
fn overindented_fences_keep_the_existing_closer_and_payload_rules() {
    for (source, expected) in [
        // A CLOSED empty payload publishes no character (markup-carve/carve#2560,
        // corpus 524, carve-rs#2162). The shape here is the over-indented closer,
        // not the payload.
        ("- a\n  - b\n %% c\n      ```\n    ```\n", "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<pre><code></code></pre>"),
        ("- a\n  - b\n %% c\n      ```\n     x\n", "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<pre><code>x\n</code></pre>"),
    ] {
        assert_html(source, expected);
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3525"]
fn comment_followups_match_the_current_reference() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/comment_followup_reference.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_html(
            row["source"].as_str().unwrap(),
            row["html"].as_str().unwrap(),
        );
    }
}
