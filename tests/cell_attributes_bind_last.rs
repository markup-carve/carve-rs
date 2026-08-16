//! A cell's attribute block binds AFTER the kind and alignment markers (spec
//! PART 9 §5 T10, PART 2 `header_cell` / `data_cell`). One order, both
//! productions: `=`, then the alignment marker, then the block, glued to
//! whatever precedes it.
//!
//! Binding it ahead of the markers instead left an attributed HEADER cell with
//! no spelling at all. The only shape available, `|{#x}=R|`, is ambiguous by
//! construction and this grammar reads it as a data cell whose content starts
//! with `=`, so the canonical writer's own output for `<th id="x">R</th>` came
//! back as `<td id="x">=R</td>` and the PART 11 §1 round-trip invariant failed
//! on it (markup-carve/carve#1226, markup-carve/carve-rs#991). Corpus category
//! 319-cell-attributes-bind-after-the-kind-and-alignment-markers pins the six
//! documents; these tests keep the reasons scoped.

fn round_trips(src: &str) -> bool {
    carve::to_html(src) == carve::to_html(&carve::to_carve(src))
}

#[test]
fn a_block_after_the_kind_marker_is_the_header_cells_attributes() {
    let html = carve::to_html("|={.total} Total |= 99 |\n| a | b |");
    assert!(
        html.contains(r#"<th scope="col" class="total">Total</th>"#),
        "got {html}"
    );
}

#[test]
fn a_block_after_the_alignment_marker_is_the_cells_attributes() {
    let header = carve::to_html("|=~{#score} Score |\n| 9 |");
    assert!(
        header.contains(r#"<th scope="col" id="score" style="text-align: center;">Score</th>"#),
        "got {header}"
    );

    let data = carve::to_html("|= Item |= Cost |\n| Pen |>{.num} 9 |");
    assert!(
        data.contains(r#"<td class="num" style="text-align: right;">9</td>"#),
        "got {data}"
    );
}

#[test]
fn a_block_glued_to_the_pipe_still_binds_where_the_cell_has_no_marker() {
    // The unchanged half: with no marker run to follow, the block sits against
    // the opening `|`, and every order agrees on this cell.
    let html = carve::to_html("|{.x} d |");
    assert!(html.contains(r#"<td class="x">d</td>"#), "got {html}");
}

#[test]
fn a_character_past_the_block_is_content_not_a_marker() {
    // The two controls. The block has taken the attribute slot, and the marker
    // slots sit BEFORE it, so neither `<` nor `=` is in a marker position any
    // more: both are literal content, and neither cell is aligned or a header.
    //
    // `|{#x}< content |` is the released spelling this rule reinterprets, and
    // it already rendered this way in every engine and in the oracle - what
    // moved is the other half, a block after a marker.
    let aligned = carve::to_html("|{#x}< content |");
    assert!(
        aligned.contains(r#"<td id="x">&lt; content</td>"#),
        "got {aligned}"
    );
    assert!(
        !aligned.contains("text-align"),
        "the `<` past the block aligned the cell: {aligned}"
    );

    let header = carve::to_html("|{#x}=R|");
    assert!(header.contains(r#"<td id="x">=R</td>"#), "got {header}");
    assert!(
        !header.contains("<th"),
        "the `=` past the block made a header cell: {header}"
    );
}

#[test]
fn a_space_in_front_of_the_block_still_makes_it_content() {
    // The validity rule and the glue rule are both unchanged: whitespace
    // anywhere ahead of the brace makes it ordinary content, in a cell with a
    // marker run exactly as in one without.
    let spaced_header = carve::to_html("|= {.x} y |");
    assert!(
        spaced_header.contains(r#"<th scope="col">{.x} y</th>"#),
        "got {spaced_header}"
    );

    let spaced_data = carve::to_html("| {.x} y |");
    assert!(spaced_data.contains("<td>{.x} y</td>"), "got {spaced_data}");
}

#[test]
fn an_invalid_payload_leaves_the_brace_literal() {
    let html = carve::to_html("|=<{.a=} y |");
    assert!(
        html.contains(r#"style="text-align: left;">{.a=} y<"#),
        "got {html}"
    );
}

#[test]
fn an_attributed_cell_is_never_a_bare_span_marker() {
    // Grammar §20: a cell carrying attributes has literal content even when
    // that content is just `^` or `<`, so the span markers do not apply after
    // the block has been read.
    let html = carve::to_html("|= A |= B |\n| a |{.x}<|");
    assert!(html.contains(r#"<td class="x">&lt;</td>"#), "got {html}");
}

#[test]
fn the_writer_emits_the_block_after_the_markers() {
    let src = carve::to_carve("|=~{#score} Score |\n| 9 |");
    assert!(
        src.contains("~{#score}"),
        "the block did not follow the alignment marker: {src}"
    );
    assert!(
        !src.contains("{#score}~"),
        "the block still leads the alignment marker: {src}"
    );
}

#[test]
fn an_attributed_header_row_no_longer_falls_back_to_a_delimiter_row() {
    // The writer had a fallback for the two header shapes the grammar could not
    // spell: it wrote the row as ordinary data cells and promoted it with a bare
    // `|---|` row. An attributed header cell is spellable now, so the fallback
    // must stop claiming it - otherwise the canonical form is still the shape
    // this rule exists to retire.
    let src = carve::to_carve("|={#x} R |= B |\n| 1 | 2 |");
    assert!(src.contains("|={#x} R |= B |"), "got {src}");
    assert!(!src.contains("---"), "a delimiter row was emitted: {src}");

    // A span marker promoted to a header cell is still unspellable - `span_cell`
    // is an ALTERNATIVE to `header_cell`, not a suffix of one - so that half of
    // the fallback stays.
    let spanned = carve::to_carve("{header-rows=1}\n|<| B |\n| a | b |");
    assert!(carve::to_html(&spanned) == carve::to_html("{header-rows=1}\n|<| B |\n| a | b |"));
}

#[test]
fn an_attributed_header_cell_round_trips() {
    // The defect this rule exists to fix: under the old order the writer's own
    // output for this table read back as a DATA cell whose content was `=R`.
    assert!(round_trips("|={#x} R |= B |\n| 1 | 2 |"));
    assert!(round_trips("|=~{.c} S |= B |\n| 1 | 2 |"));
    assert!(round_trips("|= A |= B |\n| 1 |>{.num} 2 |"));
}

#[test]
fn a_cell_whose_content_opens_a_valid_block_round_trips() {
    // The block now sits directly after the marker run, so a cell whose TEXT
    // begins with a valid attribute block would hand it to the reader as the
    // cell's own attributes. The writer parts them with one space, which the
    // reader trims back off as padding.
    assert!(round_trips("|= {.x} y |= B |\n| 1 | 2 |"));
    assert!(round_trips("| {.x} y |"));
}
