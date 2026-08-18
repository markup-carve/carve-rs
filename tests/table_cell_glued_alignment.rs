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
//! PART 10 §T9 independently gives each `<thead>` cell `scope="col"`.

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
fn valid_glued_markers_align_and_invalid_runs_stay_literal() {
    // A duplicate-axis run is invalid as a whole, so neither marker is
    // consumed as alignment.
    let header = carve::to_html("|=<< Note |= B |\n| 1 | 2 |");
    assert!(
        header.contains(r#"<th scope="col">&lt;&lt; Note</th>"#),
        "got {header}"
    );

    // A glued marker that is the whole content of a HEADER cell is alignment -
    // the `=` already marks the cell, so there is no span to confuse it with.
    let lone = carve::to_html("|=< |= B |\n| 1 | 2 |");
    assert!(
        lone.contains(r#"<th scope="col" style="text-align: left;"></th>"#),
        "got {lone}"
    );

    // The same complete-run fallback applies in a data cell.
    let data = body_row("|<<|");
    assert!(data.contains(r#"<td>&lt;&lt;</td>"#), "got {data}");
}

#[test]
fn a_vertical_marker_needs_a_horizontal_partner() {
    let html = carve::to_html(
        "|=^ Top |=v Bottom |=<^ Paired |=v> Reverse |=~> Middle |\n| a | b | c | d | e |",
    );
    assert!(html.contains(r#"<th scope="col">^ Top</th>"#), "got {html}");
    assert!(
        html.contains(r#"<th scope="col">v Bottom</th>"#),
        "got {html}"
    );
    assert!(
        html.contains("text-align: left; vertical-align: top;"),
        "got {html}"
    );
    assert!(
        html.contains("text-align: right; vertical-align: bottom;"),
        "got {html}"
    );
    assert!(
        html.contains("text-align: right; vertical-align: middle;"),
        "got {html}"
    );
    assert!(
        carve::to_carve("|=~> Middle |\n| e |\n").contains("|=>~ Middle |"),
        "the canonical form writes horizontal before vertical"
    );
}

#[test]
fn question_mark_inherits_horizontal_alignment_only() {
    let source = "|=>^ H |\n|?v x |\n";
    let html = carve::to_html(source);
    assert!(
        html.contains(r#"<td style="text-align: right; vertical-align: bottom;">x</td>"#),
        "got {html}"
    );
    assert!(
        carve::to_carve(source).contains("|?v x |"),
        "the writer keeps the vertical-only cell explicit"
    );

    for (source, visible) in [
        ("| ? |", "<td>?</td>"),
        ("|v? x |", "<td>v? x</td>"),
        ("|?< x |", "<td>?&lt; x</td>"),
    ] {
        let html = carve::to_html(source);
        assert!(html.contains(visible), "got {html}");
    }
}
