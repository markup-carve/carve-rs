//! A fence after a quote is opaque to the definition prepass.

#[test]
fn fence_after_quote_keeps_definition_shaped_payload() {
    let html = carve::to_html("- [^a]: p0\n  > q\n    ```x\n    [^z]: code\n\nsee[^a]\n");
    assert!(
        html.contains("<pre><code class=\"language-x\">[^z]: code\n\n</code></pre>"),
        "{html}"
    );
    assert!(!html.contains("%%\u{e000}"), "{html}");
}
