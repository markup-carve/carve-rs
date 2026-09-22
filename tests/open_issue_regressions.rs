use carve::{parse, to_html, to_json};

#[test]
fn a_caption_tag_is_not_a_number_placeholder() {
    for tag in ["#1", "#_", "#-a"] {
        let html = to_html(&format!("![p](p.png)\n^ Figure {tag} q\n"));
        assert!(
            html.contains(&format!(
                "<span class=\"tag\"><strong>{tag}</strong></span>"
            )),
            "{tag} was numbered: {html}"
        );
    }
}

#[test]
fn a_no_break_space_is_content_in_a_heading_reference_key() {
    for source in ["# a\u{00a0}b\n\n[a b][]\n", "# \u{00a0}ab\n\n[ab][]\n"] {
        let html = to_html(source);
        assert!(!html.contains("<a href=\"#"), "reference resolved: {html}");
    }
}

#[test]
fn a_form_feed_after_the_combined_opener_is_content() {
    assert_eq!(
        to_html("/*\u{000c}x*/ y\n"),
        "<p><strong><em>\u{000c}x</em></strong> y</p>"
    );
}

#[test]
fn a_reference_label_may_hold_an_open_bracket() {
    let source = "a [t][a[b] b\n";
    let json = to_json(&parse(source));
    assert!(
        json.contains("\"type\":\"link\""),
        "no reference node: {json}"
    );
    assert!(
        json.contains("\"rawRef\":\"[t][a[b]\""),
        "wrong raw reference: {json}"
    );
}

#[test]
fn a_reference_label_ignores_opaque_closing_brackets() {
    for source in [
        "[t][a\\]b] tail\n",
        "[t][a `]` b] tail\n",
        "![t][a{# ] #}b] tail\n",
    ] {
        let json = to_json(&parse(source));
        assert!(
            json.contains("\"rawRef\":"),
            "reference did not parse: {json}"
        );
        assert!(json.contains("tail"), "reference consumed its tail: {json}");
    }
}

#[test]
fn a_line_comment_consumes_bare_emphasis_closers() {
    for (source, visible) in [
        ("*a %% b* y\n", "<p>*a</p>"),
        ("_a %% b_ y\n", "<p>_a</p>"),
        ("/a %% b/ y\n", "<p>/a</p>"),
    ] {
        assert_eq!(to_html(source), visible);
    }
}

#[test]
fn a_comment_in_the_combined_token_ends_at_its_closer() {
    assert_eq!(
        to_html("/*a %% b*/ y\n"),
        "<p><strong><em>a</em></strong> y</p>"
    );
}

#[test]
fn a_fence_closer_must_be_at_the_items_content_column() {
    let source = "- a\n  ```\n  b\n y\n ```\n";
    let html = to_html(source);
    assert_eq!(html, "<ul>\n  <li>a\n<code>\nb\ny\n</code></li>\n</ul>");
}

#[test]
fn a_fence_closer_past_a_below_column_line_still_opens_the_block() {
    let source = "- a\n  ```\n  b\n y\n  ```\n";
    let html = to_html(source);
    assert!(
        html.contains("<pre><code>b\n</code></pre>"),
        "fence stayed inline: {html}"
    );
    assert!(
        html.ends_with("<p>y\n<code></code></p>"),
        "wrong tail: {html}"
    );
}

#[test]
fn an_item_fence_is_not_reinterpreted_after_a_blockquote() {
    let source = "- > q\n  ```\n  b\n y\n  ```\n";
    let html = to_html(source);
    assert_eq!(
        html,
        "<ul>\n  <li>\n    <blockquote><p>q</p></blockquote>\n    <pre><code>b\n</code></pre>\n  </li>\n</ul>\n<p>y\n<code></code></p>"
    );
}

#[test]
fn an_unclosed_flush_fence_stays_in_the_definition_paragraph() {
    let html = to_html(":: t\n: a\n```\n");
    assert_eq!(
        html,
        "<dl>\n  <dt>t</dt>\n  <dd>a\n<code></code></dd>\n</dl>"
    );
}

#[test]
fn a_definition_fence_uses_a_closer_past_a_below_column_line() {
    let source = ":: t\n: a\n  ```\n  b\n y\n  ```\n";
    assert_eq!(
        to_html(source),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>a</p>\n    <pre><code>b\n</code></pre>\n  </dd>\n</dl>\n<p>y\n<code></code></p>"
    );
}

#[test]
fn a_below_column_line_ends_an_empty_div_after_a_paragraph() {
    let source = "- a\n  ::: d\n y\n  :::\n";
    let html = to_html(source);
    assert_eq!(
        html,
        "<ul>\n  <li>a\n    <div class=\"d\">\n\n    </div>\n  </li>\n</ul>\n<p>y\n:::</p>"
    );
}

#[test]
fn a_nested_items_bare_colon_run_stays_in_that_item() {
    let source = "- x\n  - ::: d\n   y\n    :::\n";
    let html = to_html(source);
    assert_eq!(
        html,
        "<ul>\n  <li>x\n    <ul>\n      <li>::: d\ny\n        <div>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}
