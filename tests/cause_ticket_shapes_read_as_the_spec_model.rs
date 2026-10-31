//! Shapes from cause tickets that carve-rs already reads as the spec model.
//! Each test names its ticket; the expected HTML is the model's.

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim_end().to_string()
}


/// carve-rs#3517
#[test]
fn a_term_led_sublist_keeps_its_terms_and_lazy_column() {
    assert_eq!(
        html("- * a\n  :: u\n  :: v\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n    <dl>\n      <dt>u</dt>\n      <dt>v</dt>\n    </dl>\n  </li>\n</ul>"
    );
    assert_eq!(
        html("- * a\nz\n\n    x\n"),
        "<ul>\n  <li>\n    <ul>\n      <li><p>a\nz</p>\n        <p>x</p>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        html("> - :: t\n>    :: u\n> : e\n"),
        "<blockquote>\n  <ul>\n    <li>\n      <dl>\n        <dt>t</dt>\n        <dt>u</dt>\n      </dl>\n    </li>\n  </ul>\n  <p>: e</p>\n</blockquote>"
    );
}

/// carve#4447
#[test]
fn a_refused_plus_before_an_attribute_run_stays_text() {
    assert_eq!(
        html(":: t\n: a\n+\n{x}\n: b\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>a\n+</dd>\n</dl>\n<p x=\"\">: b</p>"
    );
}

/// carve#4180
#[test]
fn an_indented_second_term_keeps_one_description_list() {
    assert_eq!(
        html(":: t\n :: u\n: d\n : e\n"),
        "<dl>\n  <dt>t</dt>\n  <dt>u</dt>\n  <dd>d</dd>\n  <dd>e</dd>\n</dl>"
    );
}

/// carve#4504
#[test]
fn a_div_closer_at_a_note_items_content_column_closes_the_div() {
    assert_eq!(
        html("[^f]: - :::::\n        :::::\n\n[^f]\n"),
        "<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>\n          <div>\n\n          </div>\n        </li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}
