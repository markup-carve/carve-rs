// PART 0 S1 (carve#5539): a fence nested in a block a description's or note's `+`
// attached is not ended by a blank run (carve-rs#4147).

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn a_description_attached_list_keeps_its_fence_payload() {
    check(
        ":: t\n: +\n- li\n  ```\n  +\n\n  more\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>li\n        <pre><code>+\n\nmore\n</code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>",
    );
}

#[test]
fn a_line_left_of_the_fence_still_leaves_it() {
    check(
        ":: t\n: +\n- li\n  ```\n  x\n\nout\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>li\n        <pre><code>x\n\n</code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>\n<p>out</p>",
    );
}

#[test]
fn a_note_attached_list_keeps_its_fence_payload() {
    check(
        "[^a]: +\n%% c\n- li\n  ```\n  x\n\n  more\n\nsee[^a]\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>li\n          <pre><code>x\n\nmore\n\n</code></pre>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">\u{21a9}</a></p>\n    </li>\n  </ol>\n</section>",
    );
}
