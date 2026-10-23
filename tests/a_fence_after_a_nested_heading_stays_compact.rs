#[test]
fn a_fence_after_a_nested_heading_stays_compact() {
    let source = "- a\n    - # h\n  ~~~\nafter\n";
    let expected = "- a\n  - # h\n  ```\n  ```\n\nafter\n";
    let formatted = carve::to_carve(source);
    assert_eq!(formatted, expected);
    assert_eq!(carve::to_html(&formatted), carve::to_html(source));
    assert_eq!(carve::to_carve(&formatted), formatted);
}
