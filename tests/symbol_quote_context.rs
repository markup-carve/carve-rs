#[test]
fn quote_direction_uses_the_symbol_output() {
    for source in [":a:\"", "x :a:\""] {
        let expected = if source.starts_with('x') {
            "<p>x :a:“</p>"
        } else {
            "<p>:a:“</p>"
        };
        assert_eq!(carve::to_html(source).trim(), expected);
    }
    assert_eq!(carve::to_html(":a:'").trim(), "<p>:a:‘</p>");
    assert_eq!(
        carve::to_html("x :a:{.c}\"").trim(),
        "<p>x <span class=\"c\">:a:</span>”</p>"
    );

    let mut options = carve::Options::new();
    options.symbols.insert("a".into(), "x".into());
    assert_eq!(
        carve::to_html_with_options(":a:\"", &options).trim(),
        "<p>x”</p>"
    );
    options.symbols.insert("a".into(), "(".into());
    assert_eq!(
        carve::to_html_with_options(":a:\"", &options).trim(),
        "<p>(“</p>"
    );
}
