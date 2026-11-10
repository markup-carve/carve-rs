use carve::{parse_with_options, render_html, to_html, Options};

fn assert_all_routes(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let source_attribute = regex::Regex::new(r#" data-source-line="[0-9]+""#).unwrap();
    for (positions, source_lines) in [(true, false), (false, true), (true, true)] {
        let options = Options {
            positions,
            source_lines,
            ..Options::default()
        };
        let html = render_html(&parse_with_options(source, &options)).unwrap();
        assert_eq!(source_attribute.replace_all(&html, ""), expected);
    }
}

#[test]
fn a_marker_line_comment_keeps_a_reached_marker_opaque() {
    assert_all_routes("- {% a\n   - m\n   z %} y", "<ul>\n  <li> y</li>\n</ul>");
    assert_all_routes(
        "- {% a\n  - m\n  z %} y",
        "<ul>\n  <li>{% a\n    <ul>\n      <li>m\nz %} y</li>\n    </ul>\n  </li>\n</ul>",
    );
    assert_eq!(
        to_html("- a0\n  - {% a\n     - m\n     z %} y"),
        "<ul>\n  <li>a0\n    <ul>\n      <li> y</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_nested_marker_line_run_classifies_its_fence_follower() {
    assert_eq!(
        to_html("- a\n  - {% a\n    {lang=de}\n    z %} y\n    ```\n    code\n    ```"),
        "<ul>\n  <li>a\n    <ul>\n      <li>{% a\n        <p lang=\"de\">z %} y</p>\n        <pre><code>code\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        to_html("- a\n  - {% a\n     ```\n     z %} y\n    ```\n    code\n    ```"),
        "<ul>\n  <li>a\n    <ul>\n      <li> y\n        <pre><code>code\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_marker_line_run_releases_forms_below_its_content_column() {
    assert_eq!(
        to_html("- {% a\n - m\n z %} y"),
        "<ul>\n  <li>{% a</li>\n  <li>m\nz %} y</li>\n</ul>"
    );
    assert_eq!(
        to_html("- {% a\n {lang=de}\n z %} y\n  ```\n  code\n  ```"),
        "<ul>\n  <li>{% a</li>\n</ul>\n<p lang=\"de\">z %} y</p>\n<pre><code>code\n</code></pre>"
    );
    assert_eq!(
        to_html("- {% a\n ```\n z %} y\n  ```\n  code\n  ```"),
        "<ul>\n  <li>{% a</li>\n</ul>\n<pre><code>z %} y\n ```\n code\n ```</code></pre>"
    );
}

#[test]
fn an_ancestor_does_not_protect_a_fence_after_a_child_run_ended() {
    assert_eq!(
        to_html("- a0\n  - a1\n    - {% a\n     {lang=de}\n     z %} y\n      ```\n      code\n      ```"),
        "<ul>\n  <li>a0\n    <ul>\n      <li>a1\n        <ul>\n          <li>{% a</li>\n        </ul>\n        <p lang=\"de\">z %} y</p>\n        <pre><code>code\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn quoted_marker_line_runs_use_columns_after_quote_prefixes() {
    assert_all_routes(
        "> - {% a\n>    - m\n>    z %} y",
        "<blockquote>\n  <ul>\n    <li> y</li>\n  </ul>\n</blockquote>",
    );
    assert_all_routes(
        "- > {% a\n  >  - m\n  >  z %} y",
        "<ul>\n  <li>\n    <blockquote><p> y</p></blockquote>\n  </li>\n</ul>",
    );
}

#[test]
fn a_marker_shaped_payload_does_not_replace_the_comment_opener() {
    assert_all_routes(
        "- a\n  - {% note\n     - {x}\n     ```\n     text %} end",
        "<ul>\n  <li>a\n    <ul>\n      <li>{% note\n        <ul>\n          <li></li>\n        </ul>\n        <pre><code>text %} end</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
    assert_all_routes(
        "- {% note\n   - {x}\n  end %} text\n  ```\n  code\n  ```",
        "<ul>\n  <li>{% note\n    <ul>\n      <li></li>\n    </ul>\n    end %} text\n    <pre><code>code\n</code></pre>\n  </li>\n</ul>",
    );
}
