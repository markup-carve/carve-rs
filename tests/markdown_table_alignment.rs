//! The Markdown separator row is the only place a Markdown table can express
//! alignment, and COLUMN alignment is declared on the HEADER cells -- that is
//! where `|=> Age` puts it, and the HTML renderer applies it to every cell in the
//! column.
//!
//! This renderer read the first NON-header row instead, where `align` is set only
//! by a per-cell override. So ordinary aligned tables lost their alignment
//! outright, and a table with one overridden cell reported that cell's alignment
//! as the whole column's (carve#352, corpus 48/49/52/53).

fn separator_row(src: &str) -> String {
    carve::to_markdown(src)
        .lines()
        .nth(1)
        .unwrap_or_default()
        .to_string()
}

#[test]
fn right_and_center_come_from_the_header() {
    assert_eq!(
        separator_row("|= Name |=> Age |=~ City |\n| a | 1 | x |\n"),
        "| --- | ---: | :---: |"
    );
}

#[test]
fn an_invalid_doubled_marker_does_not_align_the_column() {
    assert_eq!(
        separator_row("|=<< Note |= Plain |\n| a | b |\n"),
        "| --- | --- |"
    );
}

#[test]
fn a_per_cell_override_does_not_speak_for_the_column() {
    // The header says right; one body cell overrides to left. Markdown cannot
    // express a per-cell override, so the column keeps what the header declared.
    let src = "|= Item |=> Qty |\n| Apple | 12 |\n| Subtotal |< 12 |\n";
    assert_eq!(separator_row(src), "| --- | ---: |");
}

#[test]
fn alignment_survives_a_colspan_in_the_table() {
    let src = "|=> Category |= Item |= Price |\n| Fruit | Apple | $1 |\n| Total | < | $1.50 |\n";
    assert_eq!(separator_row(src), "| ---: | --- | --- |");
}

#[test]
fn nothing_aligned_gives_plain_separators() {
    assert_eq!(separator_row("|= A |= B |\n| 1 | 2 |\n"), "| --- | --- |");
}
