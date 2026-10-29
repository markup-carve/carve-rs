use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_definition_sibling_returns_to_its_authored_item_column() {
    let source = "- - - :: t\n   : a\n    :: sibling\n    tail\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <dl>\n              <dt>t\n: a</dt>\n            </dl>\n          </li>\n        </ul>\n        <dl>\n          <dt>sibling\ntail</dt>\n        </dl>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn a_below_column_term_shape_stays_in_the_open_term() {
    let source = "- * :: t\n : a\n :: sibling\n tail\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t\n: a</dt>\n        </dl>\n      </li>\n    </ul>\n  </li>\n</ul>\n<dl>\n  <dt>sibling\ntail</dt>\n</dl>";
    assert_eq!(to_html(source), expected);
}

#[test]
fn text_past_the_deepest_term_column_keeps_its_residual_space() {
    let source = "- * :: t\n : a\n     :: sibling\n     tail\n";
    let html = to_html(source);
    assert!(
        html.contains("<dt>t\n: a</dt>\n          <dt>sibling\ntail</dt>"),
        "{html}"
    );
}

#[test]
fn an_outer_item_reclaims_a_term_between_its_child_columns() {
    let source = "- host\n  - * :: t\n   : a\n   :: sibling\n   tail\n";
    let html = to_html(source);
    assert!(html.contains("<dt>t\n: a</dt>"), "{html}");
    assert!(html.contains("<dt>sibling\ntail</dt>"), "{html}");
    let sibling = html.find("<dt>sibling").unwrap();
    let child_end = html.find("</ul>\n    <dl>").unwrap();
    assert!(child_end < sibling, "{html}");
}

#[test]
fn list_markers_follow_the_ladder_columns_after_a_lazy_term_line() {
    let between = to_html("- * :: t\n : a\n   - sibling\n   tail\n");
    assert!(
        between.contains("<dt>t\n: a\n- sibling\ntail</dt>"),
        "{between}"
    );

    let at_child = to_html("- * :: t\n : a\n    - sibling\n    tail\n");
    assert!(at_child.contains("<dt>t\n: a</dt>"), "{at_child}");
    assert!(at_child.contains("</dl>\n        <ul>"), "{at_child}");
    assert!(at_child.contains("<li>sibling\ntail</li>"), "{at_child}");
}
