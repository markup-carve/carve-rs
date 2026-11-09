// CARVE-P2-035 (carve#5080, carve#5561, carve#5573): the line after a refused brace
// run is its paragraph's text whatever it opens, a fence-shaped follower keeps
// its closer, and later lines follow the ordinary rules of that paragraph.

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn a_backtick_follower_under_an_item_plus_takes_the_lazy_line() {
    check(
        "- +\n  {#i\n  .y}\n  ```\ntail\n",
        "<ul>\n  <li>{#i\n.y}\n<code>\ntail</code></li>\n</ul>",
    );
}

#[test]
fn a_backtick_follower_under_a_note_takes_the_lazy_line() {
    check(
        "[^a]: x\n\n  {#i\n  .y}\n  ```\ntail\n\nsee[^a]\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>x</p>\n      <p>{#i\n.y}\n<code>\ntail</code><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_list_after_the_backtick_follower_leaves_the_note() {
    check(
        "[^a]: +\n{.x\n.y}\n```\n- l2\n\nsee[^a]\n",
        "<ul>\n  <li>l2</li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{.x\n.y}\n<code></code><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_note_definition_follower_is_note_text() {
    check(
        "[^a]: +\n{.x\n.y}\n[^b]: z\n\nsee[^a][^b]\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a>[^b]</p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{.x\n.y}\n[^b]: z<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_quoted_note_definition_follower_is_note_text() {
    check(
        "> [^a]: +\n> {#i\n> .y}\n> [^b]: z\n\nsee[^a][^b]\n",
        "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a>[^b]</p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{#i\n.y}\n[^b]: z<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_description_marker_ends_the_protected_follower() {
    check(
        ":: t\n: +\n{.x\n.y}\n```\n: d\n```\n",
        "<dl>\n  <dt>t</dt>\n  <dd>{.x\n.y}\n<code></code></dd>\n  <dd>d</dd>\n</dl>\n<pre><code></code></pre>",
    );
}

#[test]
fn a_term_ends_the_protected_follower() {
    check(
        ":: t\n: +\n{.x\n.y}\n```\n:: u\n```\n",
        "<dl>\n  <dt>t</dt>\n  <dd>{.x\n.y}\n<code></code></dd>\n  <dt>u</dt>\n</dl>\n<pre><code></code></pre>",
    );
}

#[test]
fn a_second_opener_after_the_follower_closer_interrupts() {
    check(
        ":: t\n: +\n{.x\n.y}\n```\nwa\n```\n```\n",
        "<dl>\n  <dt>t</dt>\n  <dd>{.x\n.y}\n<code>\nwa\n</code></dd>\n</dl>\n<pre><code></code></pre>",
    );
}

#[test]
fn a_second_opener_after_the_follower_closer_interrupts_at_the_root() {
    check(
        "{.x\n.y}\n```\nwa\n```\n```\n",
        "<p>{.x\n.y}\n<code>\nwa\n</code></p>\n<pre><code></code></pre>",
    );
}

#[test]
fn a_fence_after_an_unclosed_follower_opens_in_the_item() {
    check(
        "- +\n  {.x\n  .y}\n  ```\n  ~~~\n  - inner\n  ~~~\n",
        "<ul>\n  <li>{.x\n.y}\n<code></code>\n    <pre><code>- inner\n</code></pre>\n  </li>\n</ul>",
    );
}
