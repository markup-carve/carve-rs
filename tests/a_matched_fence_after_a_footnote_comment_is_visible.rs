fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_matched_fence_below_the_note_body_column_is_visible() {
    for comment in ["%% comment", " %% comment", "  %% comment"] {
        for (open, close) in [
            ("```", "```"),
            ("~~~", "~~~"),
            ("````", "````"),
            ("```", "````"),
        ] {
            let source = format!("lead\n[^n]: note\n{comment}\n{open}\ncode\n{close}\n");
            assert_eq!(
                html(&source),
                "<p>lead</p>\n<pre><code>code\n</code></pre>",
                "{source}"
            );
        }
    }
}

#[test]
fn an_unclosed_fence_still_folds_into_the_note() {
    assert_eq!(
        html("lead\n[^n]: note\n%% comment\n```\ncode\n"),
        "<p>lead</p>\n<pre><code>code\n</code></pre>"
    );
}

#[test]
fn a_blank_separates_the_note_and_the_fence() {
    assert_eq!(
        html("lead\n[^n]: note\n%% comment\n\n```\ncode\n```\n"),
        "<p>lead</p>\n<pre><code>code\n</code></pre>"
    );
}
