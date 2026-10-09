#[test]
fn djot_attribute_ownership_survives_import() {
    for (source, expected) in [
        ("{#id} at beginning\n", "<p> at beginning</p>"),
        ("After {#id} space\n{.class}\n", "<p>After  space\n</p>"),
        ("not a [span] {#id}.\n", "<p>not a [span] .</p>"),
        ("{#id .class}\n\nA paragraph\n", "<p>A paragraph</p>"),
        ("[span]{#id}", "<p><span id=\"id\">span</span></p>"),
        ("`x`{.c}", "<p><code class=\"c\">x</code></p>"),
        (
            "<http://x.y>{.c}",
            "<p><a href=\"http://x.y\" class=\"c\">http://x.y</a></p>",
        ),
    ] {
        let converted = carve::djot_to_carve(source);
        assert_eq!(
            carve::to_html(&converted).trim(),
            expected,
            "{source}: {converted}"
        );
    }
}
