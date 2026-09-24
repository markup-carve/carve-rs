//! PART 12 §26: a spanning cell publishes its resolved extent.

/// One cell's authored marker and its resolved counts.
type CellSpans = (Option<String>, Option<usize>, Option<usize>);

fn cells(source: &str) -> Vec<Vec<CellSpans>> {
    let doc = carve::parse(source);
    for node in &doc.children {
        if let carve::ast::BlockNode::Table(t) = node {
            return t
                .rows
                .iter()
                .map(|row| {
                    row.cells
                        .iter()
                        .map(|c| {
                            (
                                c.span.map(|s| match s {
                                    carve::ast::TableCellSpan::Rowspan => "rowspan".to_string(),
                                    carve::ast::TableCellSpan::Colspan => "colspan".to_string(),
                                }),
                                c.colspan,
                                c.rowspan,
                            )
                        })
                        .collect()
                })
                .collect();
        }
    }
    panic!("no table in {source:?}");
}

#[test]
fn the_counts_sit_on_the_origin_and_the_markers_stay() {
    let grid = cells("| a | < | b |\n| ^ | c | d |\n");
    assert_eq!(grid[0][0], (None, Some(2), Some(2)));
    assert_eq!(grid[0][1], (Some("colspan".into()), None, None));
    assert_eq!(grid[1][0], (Some("rowspan".into()), None, None));
}

#[test]
fn a_cell_spanning_one_row_and_one_column_carries_no_count() {
    for cell in &cells("| a | b |\n")[0] {
        assert_eq!(cell.1, None, "colspan");
        assert_eq!(cell.2, None, "rowspan");
    }
}

#[test]
fn a_marker_that_found_no_origin_extends_nobody() {
    // T5 is total: a `^` in the first row renders as an empty cell and no
    // count moves.
    let grid = cells("| ^ | a |\n");
    assert_eq!(grid[0][0], (Some("rowspan".into()), None, None));
    assert_eq!(grid[0][1], (None, None, None));
}

#[test]
fn the_html_the_shared_walk_produces_is_unchanged() {
    let html = carve::render_html(&carve::parse("| a | < | b |\n| ^ | c | d |\n")).expect("render");
    assert!(html.contains("rowspan=\"2\""), "{html}");
    assert!(html.contains("colspan=\"2\""), "{html}");
}

#[test]
fn an_ingested_count_wins_and_is_not_recomputed() {
    // §23's rule for `blockImage`, one field over: an importer may have
    // resolved a span from markers this engine never saw.
    let json = "{\"type\":\"document\",\"children\":[{\"type\":\"table\",\"rows\":[{\"type\":\"table_row\",\"cells\":[{\"type\":\"table_cell\",\"header\":false,\"children\":[],\"colspan\":3}]}]}],\"srcByteLength\":0}";
    let doc = carve::from_json(json).expect("decode");
    let carve::ast::BlockNode::Table(t) = &doc.children[0] else {
        panic!("expected a table");
    };
    assert_eq!(t.rows[0].cells[0].colspan, Some(3));
}
