#[test]
fn non_html_renderers_show_raw_autolink_content() {
    // Email autolink shows the address, not the mailto: href.
    assert_eq!(
        carve::to_markdown("<me@example.com>").trim(),
        "[me@example.com](mailto:me@example.com)"
    );
    // URI autolink keeps its scheme.
    assert_eq!(
        carve::to_markdown("<mailto:a@b>").trim(),
        "[mailto:a@b](mailto:a@b)"
    );
    assert_eq!(
        carve::to_plain_text("<me@example.com>").trim(),
        "me@example.com"
    );
}
