#[test]
fn braced_openers_do_not_supply_bare_emphasis_closers() {
    // `{X}` is characters (CARVE-P3-014), so its `X}` closes the outer `{X`.
    for (source, html) in [
        ("{*{*}", "<p><strong>{</strong></p>"),
        ("{~{~}", "<p><s>{</s></p>"),
        ("{={=}", "<p><mark>{</mark></p>"),
        ("{_{_}", "<p><u>{</u></p>"),
    ] {
        assert_eq!(carve::to_html(source), html, "{source}");
    }
}

#[test]
fn escaped_brace_does_not_reserve_the_following_marker() {
    assert_eq!(carve::to_html("*x\\{*}"), "<p><strong>x{</strong>}</p>");
}
