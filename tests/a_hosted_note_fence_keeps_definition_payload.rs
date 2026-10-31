//! A definition-shaped line in an item-hosted note's open fence is code
//! payload, even when it reaches the note's body column (carve#4700).
//! Registered by `tests/suite/main.rs` through `suite_modules!()`.

use carve::to_html;

#[test]
fn a_note_on_an_item_marker_line_keeps_its_fence_payload() {
    let source = "- [^a]: text\n      ~~~\n    [^b]: inner\n      one\n      ~~~\n\n[^a] [^b]\n";
    let expected = "<ul>\n  <li></li>\n</ul>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> [^b]</p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>text</p>\n      <pre><code>[^b]: inner\none\n</code></pre>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source).trim(), expected);
}

#[test]
fn both_fence_spellings_keep_definitions_at_the_note_floor() {
    for marker in ["-", "1.", "- *"] {
        for fence in ["~~~", "```"] {
            let lead = format!("{marker} [^a]: text");
            let floor = lead.find("[^a]").unwrap() + 2;
            let source = format!(
                "{lead}\n{opener}{fence}\n{payload}[^b]: inner\n{payload}one\n{opener}{fence}\n\n[^a] [^b]\n",
                opener = " ".repeat(floor + 2),
                payload = " ".repeat(floor)
            );
            let html = to_html(&source);
            assert!(html.contains("<pre><code>[^b]: inner\none\n</code></pre>"), "{source:?}: {html}");
            assert!(html.contains("</a> [^b]</p>"), "{source:?}: {html}");
            assert!(!html.contains("id=\"fn2\""), "{source:?}: {html}");
        }
    }
}

#[test]
fn a_child_item_fence_uses_the_child_column() {
    let inside = to_html("- [^a]: - ~~~\n      [^b]: inner\n      ~~~\n\n[^a] [^b]\n");
    assert!(inside.contains("<pre><code>[^b]: inner\n</code></pre>"), "{inside}");
    assert!(!inside.contains("id=\"fn2\""), "{inside}");

    let outside = to_html("- [^a]: - ~~~\n    [^b]: inner\n\n[^a] [^b]\n");
    assert!(outside.contains("id=\"fn2\""), "{outside}");
    assert!(!outside.contains("<pre><code>[^b]: inner"), "{outside}");
}
