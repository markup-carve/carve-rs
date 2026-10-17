fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_document_column_line_exits_a_retained_footnote_fence() {
    for source in [
        "[^c]: para\n%% c\n```\nx\n",
        "[^c]: para\n %% c\n```\nx\n",
        "[^c]: ::: note\n      para\n%% c\n```\nx\n",
        "[^c]: ::: note\n      more\n%% c\n```\nx\n",
        "[^c]: 1. para\n %% c\n```\nx\n",
    ] {
        assert_eq!(html(source), "<p>x</p>", "{source}");
    }
}

#[test]
fn an_unclosed_fence_line_in_a_retained_item_colon_body_opens_code() {
    // PART 0 X1 puts the retained fence in the note body.
    assert_eq!(
        html("- ::: note\n  more\n %% c\n```\n"),
        "<ul><li><aside class=\"admonition note\" aria-label=\"Note\"><p>more</p><pre><code></code></pre></aside></li></ul>"
    );
}

#[test]
fn a_blank_line_remains_payload_of_a_retained_footnote_fence() {
    let rendered = carve::to_html("a[^c]\n\n[^c]: para\n%% c\n```\n  x\n\n  y\n");
    assert!(
        rendered.contains("<pre><code>  x\n\n  y\n</code></pre>"),
        "{rendered}"
    );
    assert!(!rendered.contains("<p>y</p>"), "{rendered}");
}

#[test]
fn a_closed_document_column_fence_stays_outside_the_item_colon_body() {
    assert_eq!(
        html("- ::: note\n  more\n %% c\n```\nx\n```\n"),
        "<ul><li><aside class=\"admonition note\" aria-label=\"Note\"><p>more</p></aside></li></ul><pre><code>x </code></pre>"
    );
}
