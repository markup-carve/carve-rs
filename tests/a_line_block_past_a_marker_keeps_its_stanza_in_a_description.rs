//! A line block's payload line is inline content whatever it looks like, so a
//! lazy line joins the stanza in a description's list item too (carve#4748,
//! PART 2 `line_block`).

use carve::to_html;

#[test]
fn an_attribute_shaped_payload_keeps_the_stanza_open_past_a_marker() {
    for (source, payload) in [
        (":: t\n: - ::: |\n    {.k}\nb\n", "{.k}"),
        (":: t\n: 1. ::: |\n     {.k}\nb\n", "{.k}"),
        (":: t\n: - ::: |\n    [x]: /u\nb\n", "[x]: /u"),
    ] {
        let html = to_html(source);
        assert!(
            html.contains(&format!("<p>{payload}<br>\nb</p>")),
            "{source}\n{html}"
        );
        assert!(!html.contains("<p>b</p>"), "{source}\n{html}");
    }
}
