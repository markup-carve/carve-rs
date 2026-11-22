use carve::Options;

#[test]
fn nested_list_between_item_and_description_columns_ends_the_term() {
    assert_eq!(
        carve::render("10. :: t\n  1. x\n", &Options::default()),
        "<ol start=\"10\">\n  <li>\n    <dl>\n      <dt>t</dt>\n    </dl>\n    <ol>\n      <li>x</li>\n    </ol>\n  </li>\n</ol>"
    );
    let bullet = carve::render("- :: t\n * x\n", &Options::default());
    assert!(bullet.contains("<dt>t</dt>"), "{bullet}");
    assert!(bullet.contains("<li>x</li>"), "{bullet}");
}

#[test]
fn marker_at_description_column_and_non_one_value_stay_lazy() {
    for source in ["10. :: t\n    1. x\n", "10. :: t\n  2. x\n"] {
        let html = carve::render(source, &Options::default());
        assert!(html.contains("<dt>t\n"), "{source:?}: {html}");
        assert!(!html.contains("<li>x</li>"), "{source:?}: {html}");
    }
}
