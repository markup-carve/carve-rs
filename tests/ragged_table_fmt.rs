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
