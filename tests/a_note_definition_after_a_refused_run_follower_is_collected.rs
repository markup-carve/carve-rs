// CARVE-P2-035 (carve-rs#4136): an unclosed fence after a refused brace run is
// paragraph text, so the note definitions below it are still collected.

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn a_note_definition_after_an_unclosed_follower_is_a_note() {
    check(
        "{#i\n.y}\n```\n[^b]: z\n\nsee[^b]\n",
        "<p>{#i\n.y}\n<code></code></p>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>z<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_note_definition_after_a_blank_below_the_follower_is_a_note() {
    check(
        "{#i\n.y}\n```\n\n[^b]: z\n\nsee[^b]\n",
        "<p>{#i\n.y}\n<code></code></p>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>z<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_quoted_note_definition_after_an_unclosed_follower_is_a_note() {
    check(
        "> {#i\n> .y}\n> ```\n> [^b]: z\n\nsee[^b]\n",
        "<blockquote><p>{#i\n.y}\n<code></code></p></blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>z<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_closed_follower_keeps_the_definition_as_its_text() {
    check(
        "{#i\n.y}\n```\n[^b]: z\n```\n\nsee[^b]\n",
        "<p>{#i\n.y}\n<code>\n[^b]: z\n</code></p>\n<p>see[^b]</p>",
    );
}

#[test]
fn a_fence_that_leaves_the_quote_is_no_follower() {
    check(
        "> {.x\n> .y}\n```\n[^b]: z\n\nsee[^b]\n",
        "<blockquote><p>{.x\n.y}</p></blockquote>\n<pre><code>[^b]: z\n\nsee[^b]\n</code></pre>",
    );
}

#[test]
fn a_fence_below_the_item_column_is_no_follower() {
    check(
        "- +\n  {.x\n  .y}\n```\n[^b]: z\n\nsee[^b]\n",
        "<ul>\n  <li>{.x\n.y}</li>\n</ul>\n<pre><code>[^b]: z\n\nsee[^b]\n</code></pre>",
    );
}
