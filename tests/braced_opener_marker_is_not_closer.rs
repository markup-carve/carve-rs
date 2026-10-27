#[test]
fn braced_openers_do_not_supply_bare_emphasis_closers() {
    for source in ["{*{*}", "{~{~}", "{={=}", "{_{_}"] {
        assert_eq!(
            carve::to_html(source),
            format!("<p>{source}</p>"),
            "{source}"
        );
    }
}

#[test]
fn escaped_brace_does_not_reserve_the_following_marker() {
    assert_eq!(carve::to_html("*x\\{*}"), "<p><strong>x{</strong>}</p>");
}
