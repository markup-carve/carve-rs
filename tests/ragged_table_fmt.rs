//! The canonical writer drops an authored separator row while preserving every
//! data row's cells (PART 11 §6h).
const CASES: [(&str, &str); 3] = [
    ("| ~x~ |\n| a | b |\n", "| ~x~ |\n| a | b |\n"),
    ("| |x |\n|---|\n| y |\n", "|= |= x |\n| y |\n"),
    ("| h |\n|---|\n| |x |\n", "|= h |\n| | x |\n"),
];

#[test]
fn every_row_keeps_its_cell_count() {
    for (source, expected) in CASES {
        assert_eq!(carve::to_carve(source), expected);
        assert_eq!(
            carve::to_html(&carve::to_carve(source)),
            carve::to_html(source)
        );
    }
}

#[test]
fn fmt_keeps_literal_pipe_in_table_link_destination() {
    let source = "| h |\n|---|\n| [a](/x\\|y) z |\n";
    assert_eq!(carve::to_carve(source), "|= h |\n| [a](/x\\|y) z |\n");
}
