//! A line block in a description under nested list items keeps its stanza
//! open for a lazy line after any payload line, including one shaped like an
//! attribute or a definition (carve#4757, PART 2 `line_block`).

use carve::to_html;

#[test]
fn a_definition_shaped_payload_keeps_the_stanza_at_depth_two() {
    for payload in ["{.k}", "[x]: /u", "[^2]: n"] {
        for (host, column) in [
            ("- - :: u\n    : ", 6),
            ("- - - :: u\n      : ", 8),
            ("1. - :: u\n     : ", 7),
        ] {
            let source = format!("{host}::: |\n{}{payload}\nb\n", " ".repeat(column));
            let html = to_html(&source);
            assert!(
                html.contains(&format!("<p>{payload}<br>\nb</p>")),
                "{source}\n{html}"
            );
            assert!(!html.contains("<p>b</p>"), "{source}\n{html}");
        }
    }
}
