//! Raw blocks use djot's `=FORMAT` syntax (```=html), matching carve-js /
//! carve-php. The former `raw FORMAT` keyword form was removed. The code-fence
//! language token accepts `/` so MIME-like tags stay a single token.

#[test]
fn raw_block_passes_through_matching_format() {
    assert_eq!(
        carve::to_html("```=html\n<custom-el>V</custom-el>\n```"),
        "<custom-el>V</custom-el>"
    );
}

#[test]
fn raw_block_drops_non_matching_format() {
    assert_eq!(carve::to_html("```=latex\n\\emph{x}\n```"), "");
}

#[test]
fn raw_block_accepts_leading_whitespace_before_eq() {
    assert_eq!(carve::to_html("``` =html\n<b>x</b>\n```"), "<b>x</b>");
}

#[test]
fn eq_with_space_before_format_is_not_raw() {
    // ```= html is not a raw block; the line opens an inline code span.
    assert_eq!(
        carve::to_html("```= html\n<b>x</b>\n```"),
        "<p><code>= html\n&lt;b&gt;x&lt;/b&gt;\n</code></p>"
    );
}

#[test]
fn removed_raw_keyword_form_is_not_raw() {
    assert_eq!(
        carve::to_html("```raw html\n<b>x</b>\n```"),
        "<p><code>raw html\n&lt;b&gt;x&lt;/b&gt;\n</code></p>"
    );
}

#[test]
fn language_token_accepts_slash() {
    assert_eq!(
        carve::to_html("```text/html\nx\n```"),
        "<pre><code class=\"language-text/html\">x\n</code></pre>"
    );
}

#[test]
fn language_token_accepts_leading_slash() {
    assert_eq!(
        carve::to_html("```/html\nx\n```"),
        "<pre><code class=\"language-/html\">x\n</code></pre>"
    );
}
