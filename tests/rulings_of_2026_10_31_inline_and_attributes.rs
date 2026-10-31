//! Rulings R1 to R5 of 2026-10-31 (markup-carve/carve#4511, #4512, #4513,
//! #4514, #4516), with the spec model's HTML.

use carve::to_html;

fn assert_html(source: &str, expected: &str) {
    assert_eq!(to_html(source).trim_end(), expected.trim_end(), "{source:?}");
}

#[test]
fn a_hard_break_or_escaped_space_is_a_typography_flank() {
    assert_html("\"a-\"\\\nb\n", "<p>“a-”<br>\nb</p>\n");
    assert_html("'a-'\\\nb\n", "<p>‘a-’<br>\nb</p>\n");
    assert_html("\"a-\"\\ b\n", "<p>“a-”&nbsp;b</p>\n");
    assert_html("\"a-\"\\\n", "<p>“a-”<br>\n</p>\n");
    assert_html("x --\\\ny\n", "<p>x –<br>\ny</p>\n");
    assert_html("x --\\ y\n", "<p>x –&nbsp;y</p>\n");
    // Any other escape is a word character, as before.
    assert_html("\"a-\"\\\tb\n", "<p>“a-“\\\tb</p>\n");
    assert_html("git --\\x\n", "<p>git --\\x</p>\n");
}

#[test]
fn a_note_body_and_extension_content_hold_a_comment_run() {
    assert_html(
        "^[%% c] d\n",
        "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> d</p>\n\
         <section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    \
         <li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" \
         aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>\n",
    );
    assert_html("[a %% c] d\n", "<p>[a] d</p>\n");
    assert_html(":foo[%% c] d\n", "<p><span class=\"ext-foo\"></span> d</p>\n");
}

#[test]
fn extension_content_closes_at_the_first_unescaped_bracket() {
    assert_html(
        ":foo[a\\]b]\n",
        "<p><span class=\"ext-foo\">a]b</span></p>\n",
    );
    assert_html(
        ":foo[a\\\\]b]\n",
        "<p><span class=\"ext-foo\">a\\</span>b]</p>\n",
    );
    assert_html(":foo[a\\]\n", "<p>:foo[a]</p>\n");
    assert_html(
        ":foo[\\]]{.c}\n",
        "<p><span class=\"ext-foo c\">]</span></p>\n",
    );
}

#[test]
fn a_typed_div_writes_directive_classes_first() {
    assert_html(
        "{k=v .c}\n::: note\nx\n:::\n",
        "<aside class=\"admonition note c\" k=\"v\" aria-label=\"Note\">\n  <p>x</p>\n</aside>\n",
    );
}

#[test]
fn an_id_is_not_a_url_sink() {
    assert_html(
        "{id=\"javascript:x\"}\n# H\n\n</#javascript:x>\n",
        "<section id=\"javascript:x\">\n  <h1>H</h1>\n  <p><a href=\"#javascript:x\">H</a></p>\n</section>\n",
    );
}
