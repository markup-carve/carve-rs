#[test]
fn comments_do_not_obscure_single_image_or_display_math_caption_hosts() {
    let image = "<figure>\n  <img src=\"/i\" alt=\"a\">\n  <figcaption>cap</figcaption>\n</figure>";
    for source in [
        "{%%}![a](/i)\n^ cap",
        "{% c %}![a](/i)\n^ cap",
        "![a](/i){%%}\n^ cap",
        "![a](/i) %% c\n^ cap",
    ] {
        assert_eq!(carve::to_html(source), image, "{source:?}");
    }
    let math = "<figure>\n  <p><span class=\"math display\" role=\"math\">\\[a\\]</span></p>\n  <figcaption>cap</figcaption>\n</figure>";
    for source in ["$$`a` %% c\n^ cap", "$$`a`{%%}\n^ cap", "{%%}$$`a`\n^ cap"] {
        assert_eq!(carve::to_html(source), math, "{source:?}");
    }
    assert!(!carve::to_html("![a](/i) {% c %}\n^ cap").contains("<figure>"));
    assert_eq!(
        carve::to_html("![a][missing]\n^ cap"),
        "<p>![a][missing]\n^ cap</p>"
    );
}

#[test]
fn comment_transparent_caption_hosts_work_on_marker_lines() {
    for host in [
        "- {%%}![a](/i)\n  ^ cap",
        "- $$`a`{%%}\n  ^ cap",
        "- {%%}$$`a`\n  ^ cap",
    ] {
        let html = carve::to_html(host);
        assert!(html.contains("<figure>"), "{host:?}: {html}");
        assert!(
            html.contains("<figcaption>cap</figcaption>"),
            "{host:?}: {html}"
        );
    }
}

#[test]
fn reference_images_wait_for_resolution_before_binding_the_caption() {
    let source = "{%%}![a][r]\n^ cap\n\n[r]: /i";
    assert_eq!(
        carve::to_html(source),
        "<figure>\n  <img src=\"/i\" alt=\"a\">\n  <figcaption>cap</figcaption>\n</figure>"
    );
    assert_eq!(
        carve::to_html("{%%}![a][missing]\n^ cap"),
        "<p>![a][missing]\n^ cap</p>"
    );
}
