#[test]
fn trailing_escape_after_partial_braced_openers() {
    for (source, expected) in [
        ("{*}\\", "<p>{*}<br>\n</p>"),
        ("x {*}\\", "<p>x {*}<br>\n</p>"),
        ("{/}\\", "<p>{/}<br>\n</p>"),
        ("{_}\\", "<p>{_}<br>\n</p>"),
        ("{*}\\\\", "<p>{*}\\</p>"),
    ] {
        assert_eq!(carve::to_html(source), expected, "source: {source:?}");
    }
}
