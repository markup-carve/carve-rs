use carve::{to_html, to_html_with_options, Options};

// Each shape is checked on the fast path and on the full parser, which a
// document the rule A walk moves always takes.
fn both(source: &str) -> (String, String) {
    let mut options = Options::default();
    options.source_lines = true;
    let full = to_html_with_options(source, &options);
    let mut stripped = String::new();
    let mut rest = full.as_str();
    while let Some(at) = rest.find(" data-source-line=\"") {
        stripped.push_str(&rest[..at]);
        let tail = &rest[at + 19..];
        rest = &tail[tail.find('"').unwrap() + 1..];
    }
    stripped.push_str(rest);
    (
        to_html(source).trim().to_string(),
        stripped.trim().to_string(),
    )
}

fn check(source: &str, expected: &str) {
    let (fast, full) = both(source);
    assert_eq!(fast, expected, "fast path");
    assert_eq!(full, expected, "full parser");
}

#[test]
fn a_flush_separator_after_a_quoted_items_terms_is_a_paragraph() {
    check(
        "> - :: t\n>    :: u\n> : e\n",
        "<blockquote>\n  <ul>\n    <li>\n      <dl>\n        <dt>t</dt>\n        <dt>u</dt>\n      </dl>\n    </li>\n  </ul>\n  <p>: e</p>\n</blockquote>",
    );
}

#[test]
fn a_second_term_after_a_marker_line_sub_list_stays_in_one_list() {
    check(
        "- * a\n  :: u\n  :: v\n",
        "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n    <dl>\n      <dt>u</dt>\n      <dt>v</dt>\n    </dl>\n  </li>\n</ul>",
    );
}

#[test]
fn a_line_at_the_sub_lists_column_after_a_lazy_line_and_a_blank_stays_in_it() {
    check(
        "- * a\nz\n\n    x\n",
        "<ul>\n  <li>\n    <ul>\n      <li><p>a\nz</p>\n        <p>x</p>\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_term_led_sub_list_takes_its_column_back_after_a_lazy_line() {
    check(
        "- * :: t\n: d\n\n    x\n",
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t\n: d</dt>\n        </dl>\n        <p>x</p>\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_blank_after_a_lazy_term_line_loosens_the_outer_item() {
    check(
        "- * :: t\n: d\n\n  x\n",
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t\n: d</dt>\n        </dl>\n      </li>\n    </ul>\n    <p>x</p>\n  </li>\n</ul>",
    );
}

#[test]
fn a_hosted_notes_lazy_line_keeps_its_paragraph_open() {
    check(
        "R[^a].\n\n- [^a]: first\n more\n     - i\n",
        "<p>R<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a>.</p>\n<ul>\n  <li></li>\n</ul>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>first\nmore\n- i<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}
