use carve::{to_html, to_html_with_options, Options};

// A term is one line (carve#4861): the separator below a nested marker-line
// term is text in whichever item it reaches, and the later term opens there.

#[test]
fn a_definition_sibling_returns_to_its_authored_item_column() {
    let source = "- - - :: t\n   : a\n    :: sibling\n    tail\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <dl>\n              <dt>t</dt>\n            </dl>\n          </li>\n        </ul>\n      </li>\n    </ul>\n    : a\n    <dl>\n      <dt>sibling</dt>\n    </dl>\n    tail\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn a_below_column_separator_ends_the_lists() {
    let source = "- * :: t\n : a\n :: sibling\n tail\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t</dt>\n        </dl>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>: a</p>\n<dl>\n  <dt>sibling</dt>\n</dl>\n<p>tail</p>";
    assert_eq!(to_html(source), expected);
}

#[test]
fn a_term_past_the_deepest_term_column_after_a_separator_opens_a_new_list() {
    let source = "- * :: t\n : a\n     :: sibling\n     tail\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t</dt>\n        </dl>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>: a</p>\n<dl>\n  <dt>sibling</dt>\n</dl>\n<p>tail</p>";
    assert_eq!(to_html(source), expected);
}

#[test]
fn an_outer_item_reclaims_a_term_between_its_child_columns() {
    let source = "- host\n  - * :: t\n   : a\n   :: sibling\n   tail\n";
    let expected = "<ul>\n  <li>host\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <dl>\n              <dt>t</dt>\n            </dl>\n          </li>\n        </ul>\n      </li>\n    </ul>\n    : a\n    <dl>\n      <dt>sibling</dt>\n    </dl>\n    tail\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
}

#[test]
fn list_markers_after_a_below_column_separator_are_paragraph_text() {
    for source in [
        "- * :: t\n : a\n   - sibling\n   tail\n",
        "- * :: t\n : a\n    - sibling\n    tail\n",
    ] {
        assert_eq!(
            to_html(source),
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t</dt>\n        </dl>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>: a\n- sibling\ntail</p>",
            "{source:?}"
        );
    }
}
