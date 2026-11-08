//! An attached fence keeps payload at its content column, and a later list
//! marker receives its own refused continuation marker (carve#5507, #5508).

fn compact(source: &str) -> String {
    carve::to_html(source)
        .split('\n')
        .map(str::trim)
        .collect::<String>()
}

#[test]
fn a_plus_at_an_attached_quote_fences_content_column_is_payload() {
    for fence in ["```", "~~~"] {
        let source = format!("> +\n>  {fence}\n>  +\n");
        assert_eq!(
            compact(&source),
            "<blockquote><blockquote><pre><code>+</code></pre></blockquote></blockquote>",
            "{source}"
        );
    }
}

