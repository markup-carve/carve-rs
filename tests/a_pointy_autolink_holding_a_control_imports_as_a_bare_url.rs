// carve#4929: an ASCII control ends no pointy autolink (CommonMark 6.5), so `<`
// stays text and the GFM extension links the bare URL after it, `>` included.

#[test]
fn a_control_character_leaves_the_angle_run_to_the_bare_url_extension() {
    for (markdown, carve_source, html) in [
        (
            "<http://\u{7}>",
            "<[http://\u{7}>](http://%07>)",
            "<p>&lt;<a href=\"http://%07&gt;\">http://\u{7}&gt;</a></p>",
        ),
        (
            "<http://\u{7}a.b>",
            "<[http://\u{7}a.b>](http://%07a.b>)",
            "<p>&lt;<a href=\"http://%07a.b&gt;\">http://\u{7}a.b&gt;</a></p>",
        ),
        (
            "x <https://a.b\u{1f}> y",
            "x <[https://a.b\u{1f}>](https://a.b%1F>) y",
            "<p>x &lt;<a href=\"https://a.b%1F&gt;\">https://a.b\u{1f}&gt;</a> y</p>",
        ),
    ] {
        let imported = carve::markdown_to_carve(markdown);
        assert_eq!(imported.trim_end(), carve_source, "{markdown:?}");
        assert_eq!(carve::to_html(&imported).trim_end(), html, "{markdown:?}");
    }
}

#[test]
fn a_pointy_autolink_without_a_control_keeps_its_angle_form() {
    assert_eq!(
        carve::markdown_to_carve("<http://a.b>").trim_end(),
        "<http://a.b>"
    );
}
