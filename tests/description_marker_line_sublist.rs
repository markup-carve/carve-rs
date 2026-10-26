#[test]
fn marker_line_sublist_in_description_keeps_following_siblings() {
    for source in [
        ":: t\n: - - a\n    - b\n",
        ":: t\n: * - a\n    - b\n",
        ":: t\n: - - a\n    - b\n    - c\n",
        ":: t\n: - - - a\n      - b\n",
    ] {
        let html = carve::to_html(source);
        assert!(html.contains("<li>a</li>"), "{source:?}: {html}");
        assert!(html.contains("<li>b</li>"), "{source:?}: {html}");
        if source.contains("- c") {
            assert!(html.contains("<li>c</li>"), "{source:?}: {html}");
        }
        assert!(!html.contains("a\n- b"), "{source:?}: {html}");
    }
}

#[test]
fn marker_below_the_nested_column_stays_in_the_paragraph() {
    let html = carve::to_html(":: t\n: - - a\n   - b\n");
    assert!(html.contains("a\n- b"), "{html}");
}
