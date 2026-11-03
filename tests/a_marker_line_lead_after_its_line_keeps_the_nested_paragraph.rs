//! A nested item opened on its parent's marker line: a definition lead opens
//! no paragraph, but a block collected below it can take a lazy line, and a
//! quoted fence on the parent's marker line ends with its quote (pack
//! lazy-fec6f2fc03b0, fences-5e87d36367a5).

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn a_lazy_line_folds_into_a_quote_below_a_definition_lead() {
    assert_eq!(
        html("- - [d]: u\n    > q\nafter\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <blockquote><p>q\nafter</p></blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_lazy_line_folds_into_a_sub_item_below_a_note_lead() {
    let out = html("- - [^n]: text\n     - q\nafter\n");
    assert!(out.contains("<li>q\nafter</li>"), "{out}");
    assert!(!out.contains("<p>after</p>"), "{out}");
}

#[test]
fn a_definition_lead_alone_takes_no_lazy_line() {
    let out = html("- - [d]: u\nafter\n");
    assert!(out.ends_with("<p>after</p>"), "{out}");
}

#[test]
fn a_quoted_fence_on_the_marker_line_ends_with_its_quote() {
    assert_eq!(
        html("- > ```\n  1. > x\n       > y\n"),
        "<ul>\n  <li>\n    <blockquote>\n      <pre><code></code></pre>\n    </blockquote>\n    <ol>\n      <li>\n        <blockquote><p>x\ny</p></blockquote>\n      </li>\n    </ol>\n  </li>\n</ul>"
    );
}

#[test]
fn a_quoted_fence_keeps_its_payload_while_the_quote_continues() {
    let out = html("- > ```\n  >   x\n  > ```\n");
    assert!(out.contains("<pre><code>  x\n</code></pre>"), "{out}");
}
