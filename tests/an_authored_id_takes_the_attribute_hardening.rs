//! CARVE-P9-054 has no exception for `id` (markup-carve/carve#4488).
use carve::to_html;

#[test]
fn a_denied_id_value_is_blanked_and_a_clean_one_kept() {
    for (source, html) in [
        ("{id=\"javascript:x\"}\np\n", "<p id=\"\">p</p>"),
        ("{id=\"Shell: b\"}\np\n", "<p id=\"\">p</p>"),
        ("*x*{id=\"data:x\"}\n", "<p><strong id=\"\">x</strong></p>"),
        ("{.c id=\"javascript:x\"}\n::: note\nx\n:::\n", "id=\"\""),
        ("{id=\"ok\"}\np\n", "<p id=\"ok\">p</p>"),
        ("{#ok}\np\n", "<p id=\"ok\">p</p>"),
    ] {
        assert!(to_html(source).contains(html), "{source}");
    }
}

#[test]
fn a_generated_heading_id_is_not_an_authored_value() {
    assert!(to_html("# Data: x\n").contains("id=\"Data-x\""));
}
