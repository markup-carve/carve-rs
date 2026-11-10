// CARVE-P0-013 and CARVE-P9-067 (markup-carve/carve#5583): a host's own code fence
// ends at a refused `+` below its column, which is then a consumed marker.

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn an_item_fence_ends_at_a_final_plus() {
    check(
        "- +\np\n  ```\n+\n",
        "<ul>\n  <li>p\n    <pre><code></code></pre>\n  </li>\n</ul>",
    );
}

#[test]
fn a_description_fence_ends_at_a_final_plus() {
    check(
        ":: t\n: +\np\n  ```\n+\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>p</p>\n    <pre><code></code></pre>\n  </dd>\n</dl>",
    );
}

#[test]
fn a_note_fence_ends_at_a_final_plus() {
    check(
        "[^a]: +\np\n    ```\n+\n\nsee[^a]\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>p</p>\n      <pre><code></code></pre>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn the_item_keeps_its_follower_after_the_refused_plus() {
    check(
        "- +\np\n  ```\n+\n\n  more\n",
        // The refused marker keeps the next line's owner (carve#5637).
        "<ul>\n  <li><p>p</p>\n    <pre><code></code></pre>\n    <p>more</p>\n  </li>\n</ul>",
    );
}

#[test]
fn a_sibling_after_the_refused_plus_stays_in_the_list() {
    check(
        "- +\np\n  ```\n+\n\n- sib\n",
        "<ul>\n  <li><p>p</p>\n    <pre><code></code></pre>\n  </li>\n  <li><p>sib</p></li>\n</ul>",
    );
}
