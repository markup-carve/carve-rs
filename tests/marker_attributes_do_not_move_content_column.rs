fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn attribute_spelling_and_unicode_move_no_column() {
    for source in [
        "-{.x} a\n  # h\n",
        "-{.averylongclass} a\n  # h\n",
        "-{title=\"😀\"} a\n  # h\n",
        "-{title=\"😀\"} [x] a\n  # h\n",
        "1.{title=\"😀\"} a\n   # h\n",
    ] {
        assert!(html(source).contains("<h1 id=\"h\">h</h1>"), "{source}");
        let formatted = carve::to_carve(source);
        assert_eq!(html(&formatted), html(source), "{source}");
        assert_eq!(carve::to_carve(&formatted), formatted, "{source}");
    }
}

#[test]
fn the_former_full_prefix_column_is_text_inside_the_item() {
    let rendered = html("-{.x1} a\n       # h\n");
    assert!(!rendered.contains("<h1"));
    assert!(rendered.contains("# h</li>"));
}

#[test]
fn different_attribute_lengths_share_the_bare_marker_column() {
    let rendered = html("-{.x} a\n  # one\n-{.averylongclass} b\n  # two\n");
    assert!(rendered.contains("<h1 id=\"one\">one</h1>"));
    assert!(rendered.contains("<h1 id=\"two\">two</h1>"));
}
