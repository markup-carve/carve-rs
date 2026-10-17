#[test]
fn a_quoted_newline_stays_inside_one_csv_cell_without_splitting_output_rows() {
    let source = "{table}\n```csv\nName,Note\nA,\"one\ntwo\"\n```\n";
    let doc = carve::parse(source);

    for output in [
        carve::render_plain_text(&doc).unwrap(),
        carve::render_ansi(&doc).unwrap(),
    ] {
        assert!(output.contains("one two"), "{output}");
        assert!(!output.contains("one\ntwo"), "{output}");
    }
}

#[test]
fn a_blank_csv_record_has_a_canonical_quoted_field() {
    let doc = carve::parse("{table}\n```csv\na\n\n```\n");
    assert!(carve::render_carve(&doc)
        .unwrap()
        .contains("```csv\na\n\"\"\n```"));
}
#[test]
fn source_backed_csv_fields_have_original_source_spans() {
    use carve::ast::{BlockNode, InlineNode, Pos};
    let source = "{table}\n```csv\nα,\"b,c\"\n\"d\ne\",f\n```\n";
    let doc = carve::parse_with_options(source, &carve::Options::default().with_positions(true));
    let BlockNode::Table(table) = &doc.children[0] else {
        panic!("expected a table")
    };
    let slice = |pos: &Pos| -> String {
        source
            .chars()
            .skip(pos.start_offset)
            .take(pos.end_offset - pos.start_offset)
            .collect()
    };
    assert_eq!(slice(table.rows[0].pos.as_ref().unwrap()), "α,\"b,c\"");
    assert_eq!(slice(table.rows[0].cells[0].pos.as_ref().unwrap()), "α");
    let InlineNode::Text(text) = &table.rows[0].cells[1].children[0] else {
        panic!("expected text")
    };
    assert_eq!(slice(text.pos.as_ref().unwrap()), "b,c");
    assert_eq!(slice(table.rows[1].pos.as_ref().unwrap()), "\"d\ne\",f");
}
