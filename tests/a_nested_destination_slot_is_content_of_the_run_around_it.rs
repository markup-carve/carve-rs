//! A nested run's destination slot is content of the run around it
//! (CARVE-P3-001 `nested` / `altNested`), so a `]` inside it closes nothing.
//! Ports markup-carve/carve-js#3180.

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_owned()
}

#[test]
fn an_image_alt_holds_a_nested_link_whose_destination_has_a_bracket() {
    assert_eq!(
        html("![a [x](/u]) b](/i)"),
        "<img src=\"/i\" alt=\"a [x](/u]) b\">"
    );
}

#[test]
fn a_span_holds_a_nested_link_whose_destination_has_a_bracket() {
    assert_eq!(
        html("[a [x](/u]) b]{.c}"),
        "<p><span class=\"c\">a <a href=\"/u]\">x</a> b</span></p>"
    );
}

#[test]
fn the_outer_link_wins_as_without_the_bracket() {
    assert_eq!(html("[a [x](/u]) b](/i)"), html("[a [x](/u) b](/i)"));
    assert_eq!(
        html("[a [x](/u]) b](/i)"),
        "<p><a href=\"/i\">a x b</a></p>"
    );
}

#[test]
fn a_top_level_link_and_an_unclosed_outer_run_are_unchanged() {
    assert_eq!(html("[x](/u])"), "<p><a href=\"/u]\">x</a></p>");
    assert_eq!(html("[a [x](/u]"), "<p>[a [x](/u]</p>");
}

#[test]
fn the_closing_angle_of_an_autolink_forms_no_greater_or_equal_sign() {
    assert_eq!(
        html("<https://a.test>=[= b"),
        "<p><a href=\"https://a.test\">https://a.test</a><mark>[</mark> b</p>"
    );
    assert_eq!(html("</#x>=[= b"), "<p>&lt;/#x&gt;<mark>[</mark> b</p>");
    assert_eq!(html("a>=[= b"), "<p>a\u{2265}[= b</p>");
}

#[test]
fn a_highlight_between_autolinks_survives_formatting() {
    let source = "<https://a.test>{=[=}<https://a]>\n";
    let formatted = carve::render_carve(&carve::parse(source)).unwrap();
    assert_eq!(html(&formatted), html(source));
}
