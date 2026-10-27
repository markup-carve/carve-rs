use carve::to_html;

#[test]
fn paired_quote_marks_drop_from_derived_heading_ids() {
    for marks in [("„", "“"), ("‚", "‛"), ("«", "»"), ("‟", "‟")] {
        for level in 1..=6 {
            let source = format!("{} q {}2 active{}\n", "#".repeat(level), marks.0, marks.1);
            let html = to_html(&source);
            assert!(html.contains("id=\"q-2-active\""), "{source:?}: {html}");
        }
    }
}
