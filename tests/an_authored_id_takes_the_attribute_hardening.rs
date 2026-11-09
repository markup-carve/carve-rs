//! CARVE-P9-054 exempts `id` and the id slot (markup-carve/carve#4534).
use carve::to_html;

#[test]
fn an_authored_id_value_is_kept() {
    for (source, html) in [
        ("{id=\"javascript:x\"}\np\n", "<p id=\"javascript:x\">p</p>"),
        ("{id=\"Shell: b\"}\np\n", "<p id=\"Shell: b\">p</p>"),
        (
            "*x*{id=\"data:x\"}\n",
            "<p><strong id=\"data:x\">x</strong></p>",
        ),
        (
            "{.c id=\"javascript:x\"}\n::: note\nx\n:::\n",
            "id=\"javascript:x\"",
        ),
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
