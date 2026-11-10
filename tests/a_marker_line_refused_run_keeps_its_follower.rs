// CARVE-P2-035: a refused brace run that starts on a description marker line keeps
// its unclosed fence follower as paragraph text, like a run on an item marker line.

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn a_description_marker_line_run_keeps_its_follower() {
    check(
        ":: t\n: {#i\n  .y}\n  ```\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>{<span class=\"tag\"><strong>#i</strong></span>\n.y}</p>\n    <pre><code></code></pre>\n  </dd>\n</dl>",
    );
}

#[test]
fn a_description_marker_line_run_keeps_the_next_line() {
    check(
        ":: t\n: {#i\n  .y}\n  ```\n  more\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>{<span class=\"tag\"><strong>#i</strong></span>\n.y}</p>\n    <pre><code>more\n</code></pre>\n  </dd>\n</dl>",
    );
}

#[test]
fn a_note_definition_after_the_description_follower_is_a_note() {
    check(
        ":: t\n: {#i\n  .y}\n  ```\n[^b]: z\n\nsee[^b]\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>{<span class=\"tag\"><strong>#i</strong></span>\n.y}</p>\n    <pre><code></code></pre>\n  </dd>\n</dl>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>z<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_quoted_description_marker_line_run_keeps_its_follower() {
    check(
        "> :: t\n> : {.x\n>   .y}\n>   ```\n>   more\n",
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>\n      <p>{.x\n.y}</p>\n      <pre><code>more\n</code></pre>\n    </dd>\n  </dl>\n</blockquote>",
    );
}
