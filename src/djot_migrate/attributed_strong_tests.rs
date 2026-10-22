#[test]
fn attributes_do_not_close_strong_delimiters() {
    for (source, expected) in [
        (
            "a *word{#id key=\"*\"}*",
            "<p>a <strong><span id=\"id\" key=\"*\">word</span></strong></p>",
        ),
        (
            "*more words{#id key=\"*\"} here*",
            "<p><strong>more <span id=\"id\" key=\"*\">words</span> here</strong></p>",
        ),
        (
            "`*word{#id key=\"*\"}*`",
            "<p><code>*word{#id key=\"*\"}*</code></p>",
        ),
    ] {
        assert_eq!(crate::to_html(&super::djot_to_carve(source)), expected);
    }
}
