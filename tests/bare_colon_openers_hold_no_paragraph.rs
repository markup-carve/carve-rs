use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

const TWO_REFERENCES: &str = "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<p><a id=\"fnref1-2\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>a<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference 1\">↩<sup>1</sup></a> <a href=\"#fnref1-2\" role=\"doc-backlink\" aria-label=\"Back to reference 2\">↩<sup>2</sup></a></p>\n    </li>\n  </ol>\n</section>";

#[test]
fn a_column_zero_reference_escapes_a_nested_notes_bare_hardbreak_opener() {
    check(
        "[^n]: a\n  [^n]: a\n  \t::: \\\n[^n]\n\n[^n]\n",
        TWO_REFERENCES,
    );
    check(
        "[^n]: a\n  [^n]: a\n      ::: \\\n[^n]\n\n[^n]\n",
        TWO_REFERENCES,
    );
}

#[test]
fn a_line_block_twin_keeps_the_same_reading() {
    check(
        "[^n]: a\n  [^n]: a\n  \t::: |\n[^n]\n\n[^n]\n",
        TWO_REFERENCES,
    );
}

#[test]
fn a_hardbreak_payload_paragraph_still_takes_the_lazy_line() {
    check(
        "[^n]: a\n  [^n]: a\n    ::: \\\n    x\n[^n]\n\n[^n]\n",
        "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>a<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_nested_items_bare_hardbreak_opener_releases_a_column_zero_line() {
    check(
        "- a\n  - a\n    ::: \\\nb\n",
        "<ul>\n  <li>a\n    <ul>\n      <li>a\n        <div class=\"hardbreaks\">\n\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>b</p>",
    );
}

#[test]
fn a_nested_items_bare_quote_block_opener_releases_a_column_zero_line() {
    check(
        "- a\n  - a\n    ::: >\nb\n",
        "<ul>\n  <li>a\n    <ul>\n      <li>a\n        <blockquote>\n\n        </blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>b</p>",
    );
}
