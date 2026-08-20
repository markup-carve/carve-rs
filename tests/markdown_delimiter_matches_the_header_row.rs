//! PART 11 section 10b: where a delimiter row is required to promote the first
//! row to a header, that delimiter carries exactly one cell for each cell in the
//! HEADER ROW, not one for each column reached by a wider body row.
//!
//! This renderer sized the separator from the TABLE width, so a ragged table
//! emitted `| h |` over `| --- | --- |`. Neither python-markdown nor marked reads
//! that as a table -- the cell counts have to agree -- so the document published
//! as a paragraph of pipes and lost its table entirely (carve#1042).
//!
//! All three engines agreed on the wider row, which is why the cross-engine
//! render comparison scored the shape green throughout; the evidence that settles
//! it is an external reader, not another engine.

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
fn the_delimiter_does_not_widen_to_reach_a_wider_body_row() {
    // Corpus 284-a-ragged-table-keeps-each-row-s-cell-count-3: a one-cell header
    // over a two-cell body row.
    let out = lines("| h |\n|---|\n| |x |\n");
    assert_eq!(out[0], "| h |");
    assert_eq!(out[1], "| --- |");
    assert_eq!(out[2], "|  | x |");
}

#[test]
fn the_span_free_shape_is_reached_too() {
    // Written with the space that ends the marker run (§20 T11).
    let out = lines("|= a |\n| x | y |\n");
    assert_eq!(out[0], "| a |");
    assert_eq!(out[1], "| --- |");
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
fn the_header_alignment_survives_the_narrowing() {
    let out = lines("|=> h |\n| x | y |\n");
    assert_eq!(out[1], "| ---: |");
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
