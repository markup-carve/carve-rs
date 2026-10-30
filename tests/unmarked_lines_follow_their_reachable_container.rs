fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn an_attribute_past_a_quoted_items_column_keeps_its_paragraph_open() {
    assert_eq!(
        html("- > para\n    {#x}\nx\n"),
        "<ul><li><blockquote><p>para</p></blockquote></li></ul><p>x</p>"
    );
}

#[test]
#[ignore = "rule A ledger: carve-rs#3523"]
fn a_comment_below_a_description_retains_its_next_paragraph() {
    assert_eq!(
        html("1. :: t\n   : para\n%% c\ntail\n"),
        "<ol><li><dl><dt>t</dt><dd><p>para</p><p>tail</p></dd></dl></li></ol>"
    );
    assert_eq!(
        html("1. :: t\n   : para\n%% c\n2. b\n"),
        "<ol><li><dl><dt>t</dt><dd>para</dd></dl></li><li>b</li></ol>"
    );
    assert_eq!(
        html("1. :: t\n   : ```\n%% c\ntail\n"),
        "<ol><li><dl><dt>t</dt><dd><pre><code></code></pre></dd></dl></li></ol><p>tail</p>"
    );
}

#[test]
fn a_comment_below_a_colon_body_retains_its_next_paragraph() {
    assert_eq!(
        html("- ::: note\n  more\n %% c\nx\n"),
        "<ul><li><aside class=\"admonition note\" aria-label=\"Note\"><p>more</p></aside></li></ul><p>x</p>"
    );
    assert_eq!(
        html("- ::: note\n  more\n %% c\n- y\n"),
        "<ul><li><aside class=\"admonition note\" aria-label=\"Note\"><p>more</p></aside></li><li>y</li></ul>"
    );
}

#[test]
fn a_quote_marker_past_the_note_column_continues_the_note() {
    assert_eq!(
        html("- > - para\n  >   [^f]: n\n   >     more\n"),
        "<ul><li><blockquote><ul><li>para</li></ul></blockquote></li></ul>"
    );
}

#[test]
fn an_unclosed_fence_line_folds_into_a_quoted_term() {
    assert_eq!(
        html("> :: t\n```\n"),
        "<blockquote><dl><dt>t</dt></dl></blockquote><pre><code></code></pre>"
    );
}
