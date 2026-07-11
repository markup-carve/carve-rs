#[test]
fn markdown_table_preserves_column_alignment() {
    let out = carve::to_markdown("| a | b | c |\n|:--|:-:|--:|\n| 1 | 2 | 3 |\n");
    assert!(out.contains("| :--- | :---: | ---: |"), "{out}");
}
