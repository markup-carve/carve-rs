use carve::lint_carve;

fn overlaps(source: &str) -> Vec<(usize, usize, String)> {
    lint_carve(source)
        .into_iter()
        .filter(|w| w.rule == "table-column-overlap")
        .map(|w| (w.line, w.column, w.message))
        .collect()
}

#[test]
fn a_braced_alignment_overlaps_like_the_deprecated_prefix() {
    for row in ["|={>} H |", "|=> H |", "|={> .x} H |", "|={~~} H |"] {
        let source = format!("{{aligns=\"left\"}}\n{row}\n| a |\n");
        let found = overlaps(&source);
        assert_eq!(found.len(), 1, "{source:?}: {found:?}");
        assert_eq!((found[0].0, found[0].1), (1, 2), "{source:?}");
    }
}

#[test]
fn a_vertical_symbol_overlaps_only_valigns() {
    assert_eq!(overlaps("{valigns=\"top\"}\n|={?v} H |\n| a |\n").len(), 1);
    assert!(overlaps("{valigns=\"top\"}\n|={>} H |\n| a |\n").is_empty());
    assert!(overlaps("{aligns=\"left\"}\n|={?v} H |\n| a |\n").is_empty());
}

#[test]
fn a_run_without_its_space_or_a_data_cell_marker_does_not_overlap() {
    for row in ["|={>}H |", "|{>} H |", "|> H |", "|= {>} H |"] {
        let source = format!("{{aligns=\"left\"}}\n{row}\n| a |\n");
        assert!(overlaps(&source).is_empty(), "{source:?}");
    }
}

#[test]
fn the_overlap_is_per_column() {
    assert!(overlaps("{aligns=\"left\"}\n|= A |={>} B |\n| a | b |\n").is_empty());
    let found = overlaps("{aligns=\",right\"}\n|= A |={>} B |\n| a | b |\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].2.starts_with("Column 2 "), "{found:?}");
    assert_eq!(
        overlaps("{aligns=\"left,right\"}\n|={<} A |={>} B |\n| a | b |\n").len(),
        2
    );
}

#[test]
fn aligns_is_not_read_out_of_valigns() {
    let source = "{valigns=\"top\"}\n|={<^} H |\n| a |\n";
    let found = overlaps(source);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].1, 2);
    let arity: Vec<_> = lint_carve("{valigns=\"top\"}\n| a | b |\n")
        .into_iter()
        .filter(|w| w.rule == "table-column-arity")
        .map(|w| w.message)
        .collect();
    assert_eq!(arity.len(), 1, "{arity:?}");
    assert!(arity[0].starts_with("valigns "), "{arity:?}");
}

#[test]
fn an_indented_row_the_table_takes_is_read() {
    // The parser and carve-js both keep `  |=< b |` as the table's second row.
    assert_eq!(overlaps("{aligns=\"left\"}\n| a |\n  |=< b |\n").len(), 1);
    assert_eq!(
        overlaps("  {aligns=\"left\"}\n  | a |\n  |=< b |\n").len(),
        1
    );
}
