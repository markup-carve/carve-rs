#[test]
fn markdown_table_preserves_column_alignment() {
    let out = carve::to_markdown("| a | b | c |\n|:--|:-:|--:|\n| 1 | 2 | 3 |\n");
    assert!(out.contains("| :--- | :---: | ---: |"), "{out}");
}

#[test]
fn a_narrow_header_above_a_wider_body_is_padded() {
    assert_eq!(
        carve::to_markdown("| h |\n|---|\n| |x |\n"),
        "| h |  |\n| --- | --- |\n|  | x |\n"
    );
}
