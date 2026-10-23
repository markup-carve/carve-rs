#[test]
fn a_quote_below_an_empty_nested_item_uses_one_column() {
    let source = "- foo\n  1. +\n    > b\n";
    let expected = "- foo\n  1. +\n   > b\n";
    let formatted = carve::to_carve(source);
    assert_eq!(formatted, expected);
    assert_eq!(carve::to_html(&formatted), carve::to_html(source));
    assert_eq!(carve::to_carve(&formatted), formatted);
}

#[test]
fn a_code_fence_below_an_empty_nested_item_uses_one_column() {
    let source = "- foo\n  1. +\n    ```\n    x\n    ```\n";
    let expected = "- foo\n  1. +\n   ```\n   x\n   ```\n";
    let formatted = carve::to_carve(source);
    assert_eq!(formatted, expected);
    assert_eq!(carve::to_html(&formatted), carve::to_html(source));
    assert_eq!(carve::to_carve(&formatted), formatted);
}
