//! A wrapped attribute opener may sit between container columns (#4089).

#[test]
fn a_wrapped_attribute_between_definition_columns_attaches_after_the_definition() {
    assert_eq!(
        carve::to_html(":: t\n: a\n {.c\n}\npara\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>a\n{.c\n}\npara</dd>\n</dl>"
    );
}

#[test]
fn a_wrapped_attribute_between_nested_list_columns_ends_both_items() {
    assert_eq!(
        carve::to_html("- outer\n  - a\n   {.c\n  }\npara\n"),
        "<ul>\n  <li>outer\n    <ul>\n      <li>a\n{.c\n}\npara</li>\n    </ul>\n  </li>\n</ul>"
    );
}
