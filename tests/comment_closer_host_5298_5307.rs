#[test]
fn closer_below_nested_item_closes_span_before_note() {
    let source = "::: note\n- a\n  1. %%%\n     n\n %%%\n:::\n";
    assert_eq!(
        carve::to_html(source),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <ul>\n    <li>a\n      <ol>\n        <li></li>\n      </ol>\n    </li>\n  </ul>\n</aside>"
    );
}

#[test]
fn closer_below_item_quote_leaves_next_blank_with_item() {
    let source = "- a\n  > %%%\n  %%%\n\n  u\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li><p>a</p>\n    <blockquote>\n\n    </blockquote>\n    <p>u</p>\n  </li>\n</ul>"
    );
}

#[test]
fn closer_at_document_column_still_closes_nested_item_span() {
    let source = "::: note\n- a\n  - %%%\n      n\n%%%\n:::\n";
    assert_eq!(
        carve::to_html(source),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <ul>\n    <li>a\n      <ul>\n        <li></li>\n      </ul>\n    </li>\n  </ul>\n</aside>"
    );
}

#[test]
fn closer_at_document_column_restores_quote_prefix_before_item_blank() {
    let source = "- a\n  > %%%\n%%%\n\n  u\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li><p>a</p>\n    <blockquote>\n\n    </blockquote>\n    <p>u</p>\n  </li>\n</ul>"
    );
}
