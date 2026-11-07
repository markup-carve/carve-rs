//! PART 0 S4, definition-body `markers` row: a `: ` line at an item's marker
//! column folds once the description has closed (ruling on
//! markup-carve/carve#5323), and a list a `> +` attaches is the quote's, so it
//! lends the item no boundary.

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src).trim_end().to_string()
}

#[test]
fn a_quote_attached_term_lends_no_boundary_to_the_item() {
    assert_eq!(
        html("- a\n  > +\n  :: t\n\n  w\n: z\n"),
        "<ul>\n  <li><p>a</p>\n    <blockquote>\n      <dl>\n        <dt>t</dt>\n      </dl>\n    </blockquote>\n    <p>w\n: z</p>\n  </li>\n</ul>"
    );
}

#[test]
fn a_closed_quote_attached_description_lets_the_line_fold() {
    assert_eq!(
        html("- a\n  > +\n  :: t\n  : u\n  # H\n  w\n: z\n"),
        "<ul>\n  <li>a\n    <blockquote>\n      <dl>\n        <dt>t</dt>\n        <dd>u</dd>\n      </dl>\n    </blockquote>\n    <h1 id=\"H\">H</h1>\n    w\n: z\n  </li>\n</ul>"
    );
}

#[test]
fn a_closed_description_before_a_nested_list_lets_the_line_fold() {
    assert_eq!(
        html("- x\n  - a\n    :: t\n    : u\n    - n\n  : z\n"),
        "<ul>\n  <li>x\n    <ul>\n      <li>a\n        <dl>\n          <dt>t</dt>\n          <dd>u</dd>\n        </dl>\n        <ul>\n          <li>n\n: z</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_lazy_separator_keeps_no_indentation_in_a_nested_item() {
    let out = html("- x\n  - a\n    :: t\n    w\n   : z\n");
    assert!(out.contains("    w\n: z\n"), "{out}");
}
