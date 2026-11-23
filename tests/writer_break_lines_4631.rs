// carve-rs#4631: after a failed comparison the read-back check resumes at the next break only when both breaks
// end the same line of the paragraph. The line counts within the run, so where the paragraph sits in the source
// (or a supplied tree without positions) does not change the escapes, and `fmt` is a fixed point (C2-W-003).
use carve::api::{Carve, WriteOptions, decode_tree};

const WRITTEN: &str = "| =a |\n\n| \\{~x}\n~}\ni\n";

#[test]
fn a_paragraph_keeps_its_escapes_wherever_it_starts() {
    let carve = Carve::new();
    let options = WriteOptions::default();
    for input in [
        "|=a|\n| \\{~x}\n~}\ni\n",
        "|=a|\n\n| \\{~x}\n~}\ni\n",
        "|=a|\n\n\n| \\{~x}\n~}\ni\n",
        WRITTEN,
    ] {
        assert_eq!(carve.format(input, &options).unwrap(), WRITTEN, "{input:?}");
    }
    assert_eq!(
        carve.format("| \\{~x}\n~}\ni\n", &options).unwrap(),
        "| \\{~x}\n~}\ni\n"
    );
}

#[test]
fn a_tree_without_positions_takes_the_same_escapes() {
    let tree = decode_tree(
        r#"{"astVersion":"2.0","type":"document","children":[
            {"type":"table","columns":[{"type":"table_column"}],"children":[{"type":"table_body","children":[
                {"type":"table_row","children":[{"type":"table_cell","header":false,"children":[
                    {"type":"cell_paragraph","children":[{"type":"text","value":"=a"}]}]}]}]}]},
            {"type":"paragraph","children":[
                {"type":"text","value":"| {~x}"},{"type":"soft_break"},
                {"type":"text","value":"~}"},{"type":"soft_break"},{"type":"text","value":"i"}]}]}"#,
    )
    .unwrap();
    assert_eq!(
        Carve::new().write(&tree, &WriteOptions::default()).unwrap(),
        WRITTEN
    );
}
