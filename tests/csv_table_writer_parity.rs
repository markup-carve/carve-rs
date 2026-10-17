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
