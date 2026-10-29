#[test]
fn literal_quote_looking_list_lead_keeps_below_column_lines_lazy() {
    for lead in [">a", ">word", ">>text"] {
        let source = format!("- {lead}\n # h");
        let expected = format!(
            "<ul>\n  <li>{}</li>\n</ul>\n<section id=\"h\">\n  <h1>h</h1>\n</section>",
            lead.replace('>', "&gt;")
        );
        assert_eq!(carve::to_html(&source), expected, "{source}");
    }
}

#[test]
fn valid_quote_leads_still_open_quotes() {
    for lead in [">", "> a"] {
        let source = format!("- {lead}\n # h");
        assert!(carve::to_html(&source).contains("<blockquote>"), "{source}");
    }
}
