//! PART 0 S3 (carve-rs#4224): in a marker body a list marker ends the quote
//! above it, so a block at the child item's content column belongs to the child.

#[test]
fn heading_under_a_child_item_after_a_quote() {
    assert_eq!(
        carve::to_html("- p\n  > q\n  - x\n    # h\n"),
        "<ul>\n  <li>p\n    <blockquote><p>q</p></blockquote>\n    <ul>\n      <li>x\n        <h1 id=\"h\">h</h1>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn fence_under_a_quoted_child_item_after_a_quote() {
    assert_eq!(
        carve::to_html("> - p\n>   > q\n>   - x\n>     ```\n>     c\n>     ```\n"),
        "<blockquote>\n  <ul>\n    <li>p\n      <blockquote><p>q</p></blockquote>\n      <ul>\n        <li>x\n          <pre><code>c\n</code></pre>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>"
    );
}

#[test]
fn note_body_quote_ends_at_its_child_item() {
    let html = carve::to_html("[^n]: p\n  > q\n  - x\n    > r\n\nsee[^n]\n");
    assert!(html.contains("<li>x\n"), "{html}");
}
