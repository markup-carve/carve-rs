use carve::{parse_with_options, to_carve, to_html, to_html_with_options, BlockNode, Options};

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

#[test]
fn three_trailing_blanks_keep_payload_and_split_sibling_lists() {
    let source = "- head\n\n   ```\n   body\n\n\n\n- next\n";
    let html = to_html(source);
    assert!(html.contains("body\n\n\n\n</code></pre>"), "{html}");
    assert!(html.contains("</ul>\n<ul>"), "{html}");
    assert_eq!(to_html(&to_carve(source)), html);
}

#[test]
fn nested_fences_keep_trailing_blanks_at_eof() {
    for fence in ["```", "~~~"] {
        for blanks in 1..=3 {
            for depth in 1..=3 {
                let mut source = format!("- head\n\n   {fence}\n   body\n{}", "\n".repeat(blanks));
                for _ in 0..depth {
                    source = format!(
                        "- parent\n{}",
                        source
                            .lines()
                            .map(|line| format!("  {line}\n"))
                            .collect::<String>()
                    );
                }
                let html = to_html(&source);
                assert!(
                    html.contains(&format!("body\n{}</code></pre>", "\n".repeat(blanks))),
                    "{source:?}: {html}"
                );
                assert_eq!(
                    html,
                    to_html_with_options(&source, &Options::default().with_positions(true))
                );
                assert_eq!(html, to_html(&to_carve(&source)));
            }
        }
    }
}

#[test]
fn copied_blanks_do_not_extend_an_unfinished_div() {
    for (source, end) in [
        ("- ::: d\n  b\n\ntail\n", 2),
        ("> - ::: d\n>   b\n>\n> tail\n", 2),
        ("- ::: d\n\n  ```\n  b\n\n", 5),
    ] {
        let doc = parse_with_options(source, &Options::default().with_positions(true));
        let block = match &doc.children[0] {
            BlockNode::BlockQuote(quote) => &quote.children[0],
            block => block,
        };
        let BlockNode::List(list) = block else {
            panic!("expected list")
        };
        assert_eq!(list.pos.as_ref().unwrap().end_line, end);
        assert_eq!(list.items[0].pos.as_ref().unwrap().end_line, end);
        let position = match &list.items[0].children[0] {
            BlockNode::Div(div) => div.pos.as_ref(),
            BlockNode::Admonition(div) => div.pos.as_ref(),
            _ => panic!("expected container"),
        };
        assert_eq!(position.unwrap().end_line, end);
    }
}
