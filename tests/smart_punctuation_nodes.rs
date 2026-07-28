#[test]
fn smart_punctuation_renders_glyphs_but_formats_source_runs() {
    let src = "Carve is a \"post-Markdown\" language -- really...\n";

    let html = carve::to_html(src);
    assert!(html.contains("“post-Markdown”"));
    assert!(html.contains("– really…"));

    let formatted = carve::to_carve(src);
    assert!(formatted.contains("\"post\\-Markdown\""));
    assert!(formatted.contains("-- really..."));
}

#[test]
fn smart_punctuation_carve_round_trip_preserves_html() {
    for src in [
        "\"quote\" and 'quote'\n",
        "dash runs: -- --- ---- -----\n",
        "ellipsis here...\n",
    ] {
        assert_eq!(carve::to_html(&carve::to_carve(src)), carve::to_html(src));
    }
}

#[test]
fn smart_punctuation_nodes_carry_quote_glyphs_and_source_values() {
    let doc = carve::parse("\"q\" -- ...");
    let carve::BlockNode::Paragraph(paragraph) = &doc.children[0] else {
        panic!("expected paragraph");
    };
    let smart = paragraph
        .children
        .iter()
        .filter_map(|node| match node {
            carve::InlineNode::SmartPunctuation(s) => Some(s),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(smart[0].kind, "left_double_quote");
    assert_eq!(smart[0].value, "\"");
    assert_eq!(smart[0].glyph.as_deref(), Some("“"));
    assert_eq!(smart[2].kind, "en_dash");
    assert_eq!(smart[2].value, "--");
    assert_eq!(smart[3].kind, "ellipsis");
    assert_eq!(smart[3].value, "...");
    assert_eq!(smart[3].glyph, None);
}

#[test]
fn smart_punctuation_heading_ids_stay_rendered_text_based() {
    let html = carve::to_html("# Don't repeat yourself\n");
    assert!(html.contains("<section id=\"Don-t-repeat-yourself\">"));
    assert!(html.contains("<h1>Don’t repeat yourself</h1>"));
}
