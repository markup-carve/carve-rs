#[test]
fn a_blank_after_a_below_column_fence_does_not_reach_the_ended_inner_fence() {
    let source = ":: t\n: :: t\n  : ```\n    code\n   ```\n\n    b\n";
    let html = carve::to_html(source);
    assert!(html.contains("<pre><code>code\n</code></pre>"), "{html}");
    assert!(html.contains("<pre><code>\n b\n</code></pre>"), "{html}");
}

#[test]
fn below_column_fence_variant_sweep() {
    let mut checked = 0;
    for term in ["t", "term", "x1", "long", "é"] {
        for code in ["a", "code", "x1", "hello", "é"] {
            for fence in ["```", "~~~"] {
                for column in 0..4 {
                    for payload in ["b", "tail", "x1", "hello", "é"] {
                        let source = format!(
                            ":: {term}\n: :: {term}\n  : {fence}\n    {code}\n{}{fence}\n\n    {payload}\n",
                            " ".repeat(column)
                        );
                        let html = carve::to_html(&source);
                        assert!(
                            html.contains(&format!("<pre><code>{code}\n</code></pre>")),
                            "{source:?}: {html}"
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1000);
}
