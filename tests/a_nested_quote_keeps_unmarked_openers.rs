use carve::to_html;

#[test]
fn a_list_quote_keeps_an_indented_fence() {
    for column in [1, 3, 4, 5, 6, 7, 8] {
        let pad = " ".repeat(column);
        let source = format!("- a\n  > q\n{pad}```js\n{pad}c\n{pad}```\n");
        let expected = if column < 2 {
            "<ul>\n  <li>a\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>\n<pre><code class=\"language-js\">c\n</code></pre>"
        } else {
            "<ul>\n  <li>a\n    <blockquote><p>q</p></blockquote>\n    <pre><code class=\"language-js\">c\n</code></pre>\n  </li>\n</ul>"
        };
        assert_eq!(to_html(&source), expected, "column {column}");
    }
}

const OPENERS: &[(&str, &str, &str)] = &[
    (
        "```js\nc\n```",
        "\n<pre><code class=\"language-js\">c\n</code></pre>",
        "<pre><code class=\"language-js\">c\n</code></pre>",
    ),
    ("```=latex\nc\n```", "", ""),
    (
        "# h",
        "\n<section id=\"h\">\n  <h1>h</h1>\n</section>",
        "h</h1>",
    ),
    ("---", "\n<hr>", "<hr>"),
    (
        "| a | b |",
        "\n<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n  </tbody>\n</table>",
        "<td>a</td><td>b</td>",
    ),
];

fn indented(body: &str, column: usize) -> String {
    body.lines()
        .map(|line| format!("{}{line}\n", " ".repeat(column)))
        .collect()
}

#[test]
fn surviving_quotes_keep_each_opener_across_the_column_band() {
    for head in ["- a\n  > q\n", "[^1]: > q\n", ":: t\n: > q\n"] {
        for column in [1, 3, 4, 5, 6, 7, 8] {
            // Column 1 is outside a footnote body, as the separate control pins.
            if head.starts_with("[^1]") && column == 1 {
                continue;
            }
            for (body, _, block) in OPENERS {
                let source = format!("{head}{}\nx[^1]\n", indented(body, column));
                let html = to_html(&source);
                assert_eq!(
                    html,
                    carve::to_html_with_options(
                        &source,
                        &carve::Options::default().with_positions(true)
                    ),
                    "{source:?}"
                );
                assert!(
                    html.contains("<blockquote><p>q</p></blockquote>"),
                    "{source:?}\n{html}"
                );
                assert!(html.contains(block), "{source:?}\n{html}");
            }
        }
    }
}

#[test]
fn top_level_quotes_use_the_same_fold() {
    for column in 1..=8 {
        for (body, block, _) in OPENERS {
            let source = format!("> q\n{}", indented(body, column));
            assert_eq!(
                to_html(&source),
                format!("<blockquote><p>q</p></blockquote>{block}"),
                "{source:?}"
            );
        }
    }
}

#[test]
fn a_blank_or_the_hosts_exact_column_allows_a_block() {
    for head in ["- a\n  > q\n", "[^1]: > q\n", ":: t\n: > q\n"] {
        for column in 2..=8 {
            for blank in [false, true] {
                if !blank && column != 2 {
                    continue;
                }
                let separator = if blank { "\n" } else { "" };
                let source = format!(
                    "{head}{separator}{}\nx[^1]\n",
                    indented("```js\nc\n```", column)
                );
                let html = to_html(&source);
                assert!(
                    html.contains("<pre><code class=\"language-js\">c\n</code></pre>"),
                    "{source:?}\n{html}"
                );
                assert!(
                    html.contains("<blockquote><p>q</p></blockquote>"),
                    "{source:?}\n{html}"
                );
            }
        }
    }
}

#[test]
fn prose_instead_of_a_quote_still_allows_an_indented_block() {
    let source = "- a\n  b\n   ```js\n   c\n   ```\n";
    assert!(to_html(source).contains("<pre><code class=\"language-js\">c\n</code></pre>"));
}

#[test]
fn a_line_below_a_footnotes_body_column_stays_outside_the_note() {
    let source = "[^1]: > q\n ```js\n c\n ```\n\nx[^1]\n";
    let html = to_html(source);
    assert!(html.starts_with("<pre><code class=\"language-js\">c\n</code></pre>"));
    assert!(html.contains("<blockquote><p>q</p></blockquote>"));
}

#[test]
fn a_finished_quote_block_leaves_no_paragraph_to_fold_into() {
    for head in ["- a\n  > # q\n", "[^1]: > # q\n", ":: t\n: > # q\n"] {
        let source = format!("{head}   ```js\n   c\n   ```\n\nx[^1]\n");
        let html = to_html(&source);
        assert!(
            html.contains("<pre><code class=\"language-js\">c\n</code></pre>"),
            "{source:?}\n{html}"
        );
    }
}

#[test]
fn an_indented_quote_marker_is_text_in_the_surviving_quote() {
    for head in ["- a\n  > q\n", "[^1]: > q\n", ":: t\n: > q\n"] {
        let source = format!("{head}   > r\n\nx[^1]\n");
        assert!(
            to_html(&source).contains("<blockquote><p>q\nr</p></blockquote>"),
            "{source:?}"
        );
    }
}

#[test]
fn a_nested_quote_and_a_colon_host_keep_the_same_fold() {
    assert!(to_html("- a\n  > > q\n     # h\n")
        .contains("<blockquote><p>q</p></blockquote>\n    </blockquote>\n    <h1 id=\"h\">h</h1>"));
    assert!(to_html("::: note\n> q\n    # h\n:::\n")
        .contains("<blockquote><p>q</p></blockquote>\n  <h1 id=\"h\">h</h1>"));
}

#[test]
fn a_comment_fence_below_the_definition_column_retains_the_body() {
    use carve::ast::BlockNode;
    let doc = carve::parse(":: t\n: > q\n %%%\n secret\n %%%\n");
    assert!(matches!(&doc.children[0], BlockNode::DefinitionList(_)));
    assert_eq!(doc.children.len(), 1);
    assert_eq!(
        carve::to_html(":: t\n: > q\n %%%\n secret\n %%%\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote><p>q</p></blockquote>\n  </dd>\n</dl>"
    );
}
