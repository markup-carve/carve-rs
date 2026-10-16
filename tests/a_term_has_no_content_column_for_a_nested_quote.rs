fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_quote_past_an_item_term_remains_term_text() {
    assert_eq!(
        html("- :: term\n   > nested\n"),
        "<ul><li><dl><dt>term &gt; nested</dt></dl></li></ul>"
    );
}
