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

#[test]
fn refused_nested_continuation_keeps_the_blank_as_payload() {
    for lead in ["- - ", "1. - ", "- - - "] {
        for fence in ["```", "~~~", "```=html"] {
            for blanks in 1..=2 {
                let source = format!(
                    "{lead}{fence}\n{}+\n{}tail",
                    " ".repeat(lead.len() - 2),
                    "\n".repeat(blanks)
                );
                let deleted = format!("{lead}{fence}\n{}tail", "\n".repeat(blanks));
                for html in [
                    to_html(&source),
                    to_html_with_options(&source, &Options::default().with_positions(true)),
                ] {
                    assert_eq!(html, to_html(&deleted), "{source:?}");
                    if fence != "```=html" {
                        let payload = format!("<pre><code>{}</code></pre>", "\n".repeat(blanks));
                        assert!(html.contains(&payload), "{source:?}: {html}");
                    }
                    assert!(html.ends_with("<p>tail</p>"), "{source:?}: {html}");
                }
            }
        }
    }
}

#[test]
fn wide_nested_marker_uses_its_marker_column_for_continuation() {
    for lead in ["- 1. ", "- 10. ", "- - [ ] "] {
        let source = format!("{lead}```\n  +\n\ntail");
        for html in [
            to_html(&source),
            to_html_with_options(&source, &Options::default().with_positions(true)),
        ] {
            assert!(
                html.contains("<pre><code>\n</code></pre>"),
                "{source:?}: {html}"
            );
            assert!(html.ends_with("<p>tail</p>"), "{source:?}: {html}");
        }
    }
}
