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
                    let expected = if opener < column {
                        format!("<{tag}{attrs}>\n  <li>item\n    tail\n  </li>\n</{tag}>")
                    } else {
                        format!("<{tag}{attrs}>\n  <li>item</li>\n</{tag}>\n<p>tail</p>")
                    };
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
fn a_code_fence_after_a_below_column_comment_keeps_its_lazy_payload() {
    for closer in 0..=8 {
        assert_html(
            &format!(
                "- item\n %%%\n hidden\n{}%%%\n  ```\ntail\n",
                " ".repeat(closer)
            ),
            "<ul>\n  <li>item\n    <pre><code>tail\n</code></pre>\n  </li>\n</ul>",
        );
    }
}

#[test]
fn folded_code_keeps_block_markers_as_payload() {
    for payload in ["- not item", "1. not item", "%%%", "::: note", "# heading"] {
        assert_html(
            &format!("- item\n %%%\n hidden\n  %%%\n  ```\n  {payload}\n"),
            &format!("<ul>\n  <li>item\n    <pre><code>{payload}\n</code></pre>\n  </li>\n</ul>"),
        );
    }
}

#[test]
fn a_code_closer_outside_the_item_does_not_change_interruption() {
    assert_html(
        "- item\n %%%\n hidden\n  %%%\n  ```\ntail\n\n%%%\n```\n%%%\n",
        "<ul>\n  <li>item\n    <pre><code>tail\n\n</code></pre>\n  </li>\n</ul>",
    );
    assert_html(
        "- item\n %%%\n hidden\n  %%%\n  ```\ntail\n\n- other\n\n      ```\n",
        "<ul>\n  <li><p>item</p>\n    <pre><code>tail\n\n</code></pre>\n  </li>\n  <li><p>other</p>\n    <pre><code>\n</code></pre>\n  </li>\n</ul>",
    );
}

#[test]
fn a_below_column_line_comment_keeps_the_same_code_fold() {
    assert_html(
        "- item\n %% c\n  ```\ntail\n",
        "<ul>\n  <li>item\n    <pre><code>tail\n</code></pre>\n  </li>\n</ul>",
    );
}

#[test]
fn folded_code_payload_still_controls_later_lazy_continuation() {
    for payload in ["# h", "::: note", "%% c", "x\n  # h"] {
        let code = payload.replace("\n  ", "\n");
        assert_html(
            &format!("- item\n %%%\n hidden\n  %%%\n  ```\n  {payload}\ntail\n"),
            &format!("<ul>\n  <li>item\n    <pre><code>{code}\n</code></pre>\n  </li>\n</ul>\n<p>tail</p>"),
        );
    }
    for (payload, escaped) in [("- x", "- x"), ("> q", "&gt; q")] {
        assert_html(
            &format!("- item\n %% c\n  ```\n  {payload}\ntail\n"),
            &format!(
                "<ul>\n  <li>item\n    <pre><code>{escaped}\ntail\n</code></pre>\n  </li>\n</ul>"
            ),
        );
    }
}

#[test]
fn a_lazy_backtick_line_is_code_payload() {
    assert_html(
        "- item\n %%%\n hidden\n  %%%\n  ```\ntail\n```\nafter\n",
        "<ul>\n  <li>item\n    <pre><code>tail\n```\nafter\n</code></pre>\n  </li>\n</ul>",
    );
}

#[test]
fn a_nested_code_fence_after_a_below_column_comment_keeps_its_lazy_payload() {
    for fence in ["```", "~~~"] {
        for comment in [
            "%% c\n",
            " %% c\n",
            " %%%\n hidden\n %%%\n",
            " %%%\n hidden\n      %%%\n",
        ] {
            assert_html(
                &format!("- a\n  - b\n{comment}    {fence}\ntail\n"),
                "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code>tail\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>",
            );
        }
    }
}

#[test]
fn nested_comment_code_folds_use_the_authored_fence_column() {
    // Expected HTML comes from the spec renderer at 56b21217.
    for (source, expected) in [
        ("- a\n  - b\n %% c\n    ```\ntail\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code>tail\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>"),
        ("- a\n  - b\n    - c\n %% c\n      ```\ntail\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <ul>\n          <li>c\n            <pre><code>tail\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>"),
        ("- - b\n %% c\n    ```\ntail\n", "<ul>\n  <li>\n    <ul>\n      <li>b\n        <pre><code>tail\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>"),
        ("1. a\n   10. b\n %% c\n       ```\ntail\n", "<ol>\n  <li>a\n    <ol start=\"10\">\n      <li>b\n        <pre><code>tail\n</code></pre>\n      </li>\n    </ol>\n  </li>\n</ol>"),
    ] {
        assert_html(source, expected);
    }
}

#[test]
fn a_closed_nested_fence_still_ends_at_the_dedent() {
    for column in [0, 1] {
        for fence in ["```", "~~~"] {
            let closer = if fence == "```" {
                "<code></code>"
            } else {
                "~~~"
            };
            assert_html(
                &format!("- a\n  - b\n{}%% c\n    {fence}\ntail\n    {fence}\n", " ".repeat(column)),
                &format!("<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code>\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>tail\n{closer}</p>"),
            );
        }
    }
}

#[test]
fn a_blank_ends_the_nested_lazy_code_fold() {
    assert_html(
        "- a\n  - b\n %% c\n    ```\ntail\n\nend\n",
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code>tail\n\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>end</p>",
    );
}

#[test]
fn a_fence_below_the_nested_column_is_lazy_code_payload() {
    assert_html(
        "- a\n  - b\n %% c\n    ```\ntail\n  ```\nafter\n",
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code>tail\n```\nafter\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_comment_at_the_outer_column_keeps_the_closed_fence_in_the_inner_item() {
    assert_html(
        "- - b\n  %% c\n    ```\ntail\n    ```\n",
        "<ul>\n  <li>\n    <ul>\n      <li>b\n        <pre><code>\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>tail\n<code></code></p>",
    );
}

#[test]
fn blocks_at_an_ancestor_column_end_the_nested_code_fold() {
    for (suffix, expected) in [
        ("  - x\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code>tail\n</code></pre>\n      </li>\n      <li>x</li>\n    </ul>\n  </li>\n</ul>"),
        ("  > q\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code>tail\n</code></pre>\n      </li>\n    </ul>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>"),
        ("  # h\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code>tail\n</code></pre>\n      </li>\n    </ul>\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>"),
    ] {
        assert_html(&format!("- a\n  - b\n %% c\n    ```\ntail\n{suffix}"), expected);
    }
}

#[test]
fn lazy_code_between_item_columns_drops_its_placing_indent() {
    for payload in ["x", "- x"] {
        assert_html(
            &format!("- a\n  - b\n %% c\n    ```\ntail\n   {payload}\n"),
            &format!("<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code>tail\n{payload}\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>"),
        );
    }
}

#[test]
fn the_nested_code_fold_survives_explicit_quote_prefixes() {
    assert_html(
        "> - a\n>   - b\n> %% c\n>     ```\n> tail\n",
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>b\n          <pre><code>tail\n</code></pre>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>",
    );
}

#[test]
fn overindented_fences_keep_the_existing_closer_and_payload_rules() {
    for (source, expected) in [
        ("- a\n  - b\n %% c\n      ```\n    ```\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>"),
        ("- a\n  - b\n %% c\n      ```\n     x\n", "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <pre><code> x\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>"),
    ] {
        assert_html(source, expected);
    }
}
