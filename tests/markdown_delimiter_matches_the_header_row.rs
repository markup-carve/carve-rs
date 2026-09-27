//! The Markdown header row and its delimiter are as wide as the widest row
//! (PART 11 section 10n): GFM drops every body cell past the header's width, so
//! a narrower header row gains empty cells and the delimiter matches it. Body
//! rows keep their own cell counts.
//!
//! The delimiter used to stop at the header row's own width (carve#1042), which
//! kept the table but let a GFM reader drop the wider rows' extra cells.

fn lines(src: &str) -> Vec<String> {
    carve::to_markdown(src)
        .lines()
        .map(str::to_string)
        .collect()
}

fn cell_count(row: &str) -> usize {
    let parts: Vec<&str> = row.split('|').collect();
    parts.len().saturating_sub(2)
}

#[test]
fn a_narrower_header_is_padded_to_a_wider_body_row() {
    // Corpus 284-a-ragged-table-keeps-each-row-s-cell-count-3: a one-cell header
    // over a two-cell body row.
    let out = lines("| h |\n|---|\n| |x |\n");
    assert_eq!(out[0], "| h |  |");
    assert_eq!(out[1], "| --- | --- |");
    assert_eq!(out[2], "|  | x |");
}

#[test]
fn the_span_free_shape_is_reached_too() {
    // Written with the space that ends the marker run (§20 T11).
    let out = lines("|= a |\n| x | y |\n");
    assert_eq!(out[0], "| a |  |");
    assert_eq!(out[1], "| --- | --- |");
    assert_eq!(out[2], "| x | y |");
}

#[test]
fn a_header_wider_than_its_body_keeps_its_own_width() {
    // Corpus 284-a-ragged-table-keeps-each-row-s-cell-count-2: the header is the
    // wide row here, so the separator stays two cells.
    let out = lines("| |x |\n|---|\n| y |\n");
    assert_eq!(out[0], "|  | x |");
    assert_eq!(out[1], "| --- | --- |");
    assert_eq!(out[2], "| y |");
}

#[test]
fn the_header_alignment_survives_the_padding() {
    let out = lines("|=> h |\n| x | y |\n");
    assert_eq!(out[1], "| ---: | --- |");
}

#[test]
fn a_padded_column_takes_a_later_header_row_s_alignment() {
    // PART 11 section 10d: the effective alignment comes from every header row,
    // so a column only the second one reaches is still aligned.
    let out = lines("|= a |\n|= b |=> c |\n| 1 | 2 |\n");
    assert_eq!(out[0], "| a |  |");
    assert_eq!(out[1], "| --- | ---: |");
}

#[test]
fn the_delimiter_always_matches_the_header_it_promotes() {
    for src in [
        "| h |\n|---|\n| |x |\n",
        "|= a |\n| x | y |\n",
        "| |x |\n|---|\n| y |\n",
        "|= A |= B |\n| 1 | 2 |\n",
        "|=> h |\n| x | y | z |\n",
    ] {
        let out = lines(src);
        assert_eq!(
            cell_count(&out[1]),
            cell_count(&out[0]),
            "delimiter width does not match the header for {src:?}"
        );
    }
}
