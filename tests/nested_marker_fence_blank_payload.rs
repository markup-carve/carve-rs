use carve::{to_html, to_html_with_options, Options};

#[test]
fn nested_marker_fence_keeps_its_blank_payload() {
    let source = "- - ```\n\ntail";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code>\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>tail</p>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn blank_payload_survives_each_list_depth_and_follower() {
    for depth in 1..=3 {
        for follower in ["tail", "[^1]: a", "[r]: /u", "{.x}"] {
            for final_break in ["", "\n"] {
                for blanks in 1..=2 {
                    let source = format!(
                        "{}```\n{}{}{}",
                        "- ".repeat(depth),
                        "\n".repeat(blanks),
                        follower,
                        final_break
                    );
                    let payload = format!("<pre><code>{}</code></pre>", "\n".repeat(blanks));
                    for html in [
                        to_html(&source),
                        to_html_with_options(&source, &Options::default().with_positions(true)),
                    ] {
                        assert!(html.contains(&payload), "{source:?}: {html}");
                    }
                }
            }
        }
    }
}

#[test]
fn closed_nested_marker_fence_keeps_its_blank_payload() {
    let source = "- - ```\n\n    ```\ntail";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code>\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>tail</p>";
    assert_eq!(to_html(source), expected);
}
