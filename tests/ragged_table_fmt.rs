//! A separator-row source keeps its separator row: `table.separatorWidths`
//! records the author's spelling (PART 11 §6h, markup-carve/carve#2894). A row
//! with more cells than the separator has entries keeps its own cell count: the
//! row is spelled one separator cell per RECORDED width and no further, so the
//! formatter never invents an entry the author did not write.
const CASES: [(&str, &str); 3] = [
    ("| ~x~ |\n| a | b |\n", "| ~x~ |\n| a | b |\n"),
    ("| |x |\n|---|\n| y |\n", "| | x |\n|---|\n| y |\n"),
    ("| h |\n|---|\n| |x |\n", "| h |\n|---|\n| | x |\n"),
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
    assert_eq!(carve::to_carve(source), source);
}
