#[test]
fn representable_autolinks_keep_their_angle_spelling() {
    for source in [
        "<https://github.com/markup-carve>",
        "<https://example.com?a=1&b=2>",
        "<https://example.com/café>",
        "<HTTPS://EXAMPLE.COM>",
        "<user@example.com>",
        "<mailto:user@example.com>",
    ] {
        let markdown = carve::to_markdown(source);
        assert_eq!(markdown.trim_end(), source);
        assert_eq!(carve::markdown_to_carve(&markdown).trim_end(), source);
    }
}

#[test]
fn autolinks_fall_back_when_angle_spelling_changes_the_content() {
    for (source, expected) in [
        (
            "<https://example.com?a=1&copy;>",
            "[https://example.com?a=1\\&copy;](https://example.com?a=1&amp;copy;)",
        ),
        (
            "<https://example.com>{title=\"Example\"}",
            "[https://example.com](https://example.com \"Example\")",
        ),
        (
            "<https://example.com/a(b)>",
            "[https://example.com/a(b)](https://example.com/a%28b%29)",
        ),
        ("<javascript:alert1>", "[javascript:alert1]()"),
        ("<user@-x.com>", "[user@-x.com](mailto:user@-x.com)"),
    ] {
        assert_eq!(carve::to_markdown(source).trim_end(), expected, "{source}");
    }
}
