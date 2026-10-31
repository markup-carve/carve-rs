//! A list marker below a hosted note's floor folds into the note's paragraph
//! (PART 9 §10 I2) unless it sits left of a hosting item's content column, where
//! it continues that list. A description hosts no list, so there it always folds.
//! A lazy line under a term keeps the `:` line below it a description.

use carve::to_html;

const NOTE: &str = "<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>note\n- b<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
const REF: &str = "<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>";

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

#[test]
fn a_marker_under_a_description_note_folds_at_any_column() {
    let dl = "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>";
    for marker in ["- b", "  - b", "   - b", "    - b"] {
        assert_eq!(
            html(&format!(":: t\n:  [^q]: note\n{marker}\n\nx[^q]\n")),
            format!("{dl}\n{REF}\n{NOTE}"),
            "{marker:?}"
        );
    }
}

#[test]
fn a_marker_at_the_hosting_item_column_folds() {
    assert_eq!(
        html("- [^q]: note\n  - b\n\nx[^q]\n"),
        format!("<ul>\n  <li></li>\n</ul>\n{REF}\n{NOTE}")
    );
    assert_eq!(
        html("> - [^q]: note\n>   - b\n\nx[^q]\n"),
        format!("<blockquote>\n  <ul>\n    <li></li>\n  </ul>\n</blockquote>\n{REF}\n{NOTE}")
    );
}

#[test]
fn a_marker_left_of_the_hosting_item_column_continues_the_list() {
    assert_eq!(
        html("- [^q]: note\n- b\n"),
        "<ul>\n  <li></li>\n  <li>b</li>\n</ul>"
    );
    assert_eq!(
        html("- :: t\n  :  [^q]: note\n- b\n"),
        "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n      <dd></dd>\n    </dl>\n  </li>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn a_description_note_after_a_lazy_term_line_registers() {
    assert_eq!(
        html(":: t\nu\n:  [^q]: n\n\nx[^q]\n"),
        format!("<dl>\n  <dt>t\nu</dt>\n  <dd></dd>\n</dl>\n{REF}\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>n<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>")
    );
}

#[test]
fn a_marker_line_under_a_term_is_no_lazy_line() {
    assert_eq!(
        html(":: t\n    * child\n:  [^q]: note\n"),
        "<dl>\n  <dt>t</dt>\n</dl>\n<ul>\n  <li>child\n:  [^q]: note</li>\n</ul>"
    );
}

#[test]
fn a_line_leaving_a_quote_hosted_note_left_of_the_item_column_ends_the_note() {
    let note = "<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>n<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(
        html("- > [^f]: n\n- tail\n\nx[^f]\n"),
        format!("<ul>\n  <li>\n    <blockquote>\n\n    </blockquote>\n  </li>\n  <li>tail</li>\n</ul>\n{note}")
    );
    assert_eq!(
        html("- > [^f]: n\n> tail\n\nx[^f]\n"),
        format!("<ul>\n  <li>\n    <blockquote>\n\n    </blockquote>\n  </li>\n</ul>\n<blockquote><p>tail</p></blockquote>\n{note}")
    );
    assert_eq!(
        html("- - > [^f]: n\n  - tail\n\nx[^f]\n"),
        format!("<ul>\n  <li>\n    <ul>\n      <li>\n        <blockquote>\n\n        </blockquote>\n      </li>\n      <li>tail</li>\n    </ul>\n  </li>\n</ul>\n{note}")
    );
}

#[test]
fn a_line_leaving_a_quote_hosted_note_at_the_item_column_folds() {
    assert_eq!(
        html("- > [^f]: n\n  - b\n\nx[^f]\n"),
        format!(
            "<ul>\n  <li>\n    <blockquote>\n\n    </blockquote>\n  </li>\n</ul>\n{REF}\n{}",
            NOTE.replace("note\n", "n\n")
        )
    );
}
