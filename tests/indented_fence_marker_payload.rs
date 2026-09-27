use carve::{to_carve, to_html, to_html_with_options, Options};

#[test]
fn markers_inside_authored_fences_are_payload() {
    for marker in ["- payload", "1. payload", "- [x] payload", "-{.x} payload"] {
        for fence in ["```", "~~~", "```=html"] {
            for extra in [1, 4] {
                for quoted in [false, true] {
                    let pad = " ".repeat(2 + extra);
                    let source = format!("- head\n\n{pad}{fence}\n{pad}body\n  {marker}\n");
                    let source = if quoted {
                        source.lines().map(|line| format!("> {line}\n")).collect()
                    } else {
                        source
                    };
                    let html = to_html(&source);
                    let payload = format!("body\n{marker}\n");
                    assert!(html.contains(&payload), "{source:?}: {html}");
                    assert_eq!(
                        html,
                        to_html_with_options(&source, &Options::default().with_positions(true))
                    );
                    let formatted = to_carve(&source);
                    assert_eq!(html, to_html(&formatted), "{source:?}");
                    assert_eq!(formatted, to_carve(&formatted), "{source:?}");
                }
            }
        }
    }
}

#[test]
fn a_closer_at_the_authored_column_releases_the_next_marker() {
    for fence in ["```", "~~~"] {
        let source = format!("- head\n\n   {fence}\n   body\n   {fence}\n  - child\n");
        let html = to_html(&source);
        assert!(html.contains("<pre><code>body\n</code></pre>"), "{html}");
        assert!(html.contains("<li>child</li>"), "{html}");
    }
}

#[test]
fn an_open_authored_fence_stops_before_outer_text_after_a_blank() {
    for (lead, column) in [("- head", 3), ("1. head", 4)] {
        let pad = " ".repeat(column);
        let source = format!("{lead}\n\n{pad}```\n{pad}body\n\nout\n");
        let html = to_html(&source);
        assert!(html.contains("<pre><code>body\n\n</code></pre>"), "{html}");
        assert!(html.ends_with("<p>out</p>"), "{html}");
        let quoted: String = source.lines().map(|line| format!("> {line}\n")).collect();
        let quoted_html = to_html(&quoted);
        assert!(quoted_html.contains("<p>out</p>"), "{quoted_html}");
        assert!(
            quoted_html.contains("<pre><code>body\n\n</code></pre>"),
            "{quoted_html}"
        );
    }
    let html = to_html("- head\n\n   ```\n   body\n\n- next\n");
    assert!(html.contains("<li><p>next</p></li>"), "{html}");
}

#[test]
fn a_closer_between_container_and_authored_columns_is_payload() {
    let source = "- head\n\n      ```\n      body\n    ```\n  - payload\n";
    let html = to_html(source);
    assert!(
        html.contains("body\n  ```\n- payload\n</code></pre>"),
        "{html}"
    );
    let source = "- head\n\n      ```\n      body\n  ```\n  - child\n";
    assert!(to_html(source).contains("<li>child</li>"));
}
