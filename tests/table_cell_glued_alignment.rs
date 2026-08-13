//! The alignment marker is GLUED to the opening `|` (spec PART 9 §5,
//! `data_cell` / `header_cell`: "alignment_marker is glued to the opening '|'
//! (no preceding whitespace) ... a whitespace-delimited lone '^'/'<' is
//! span_cell").
//!
//! Reading the marker off the TRIMMED cell threw that distinction away, so a
//! `<` anywhere at the front of a cell's content aligned it. The reported
//! symptom was `| << |`, whose first `<` was consumed as alignment and whose
//! second then matched as a lone colspan marker, emitting an empty cell where
//! carve-js and carve-php both render literal text (carve-rs#459).

fn body_row(cell: &str) -> String {
    carve::to_html(&format!("| a | b |\n|---|---|\n{cell} d |"))
}

#[test]
fn a_doubled_colspan_marker_is_literal_text() {
    assert!(
        body_row("| << |").contains("<td>&lt;&lt;</td>"),
        "got {}",
        body_row("| << |")
    );
}

#[test]
fn a_lone_marker_is_still_a_span_cell() {
    // The one case a glued marker does not win: `|<|` is a colspan in every
    // engine, and the whitespace-delimited `| < |` is the form the corpus pins
    // (98-table-span-marker-in-first-column).
    assert!(
        body_row("|<|").contains("<td></td>"),
        "got {}",
        body_row("|<|")
    );
    assert!(
        body_row("| < |").contains("<td></td>"),
        "got {}",
        body_row("| < |")
    );
}

#[test]
fn a_marker_after_whitespace_is_content_not_alignment() {
    // carve-js and carve-php render both of these as literal `<`; carve-rs
    // aligned them, which no fixture covered.
    let data = body_row("| < x |");
    assert!(
        data.contains("<td>&lt; x</td>"),
        "a data cell aligned on a spaced marker: {data}"
    );

    let header = carve::to_html("|= <  A |= B |\n| 1 | 2 |");
    assert!(
        header.contains("<th scope=\"col\">&lt;  A</th>"),
        "a header cell aligned on a spaced marker: {header}"
    );
}

#[test]
fn a_glued_marker_still_aligns() {
    // Corpus 53-table-doubled-alignment-marker: the first `<` aligns, the
    // second is literal content.
    let header = carve::to_html("|=<< Note |= B |\n| 1 | 2 |");
    assert!(
        header.contains(r#"<th scope="col" style="text-align: left;">&lt; Note</th>"#),
        "got {header}"
    );

    // A glued marker that is the whole content of a HEADER cell is alignment -
    // the `=` already marks the cell, so there is no span to confuse it with.
    let lone = carve::to_html("|=< |= B |\n| 1 | 2 |");
    assert!(
        lone.contains(r#"<th scope="col" style="text-align: left;"></th>"#),
        "got {lone}"
    );

    // And in a DATA cell a glued marker followed by content aligns, leaving the
    // rest literal - the `span_cell` alternative does not apply once a marker
    // has been taken.
    let data = body_row("|<<|");
    assert!(
        data.contains(r#"<td style="text-align: left;">&lt;</td>"#),
        "got {data}"
    );
}
