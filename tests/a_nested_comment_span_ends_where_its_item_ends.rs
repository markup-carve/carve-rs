//! A `%%%` span a nested item holds: a closer below the item's column still
//! closes it (PART 9 §28), and a line that misses the item ends it (PART 0 S1)
//! so a blank before that line separates blocks again (carve#4907).

fn flat(source: &str) -> String {
    carve::to_html(source)
        .split('\n')
        .map(str::trim)
        .collect::<String>()
}

#[test]
fn a_closer_below_the_item_closes_a_marker_line_span() {
    for closer in ["%%%", " %%%", "  %%%", "    %%%"] {
        assert_eq!(
            flat(&format!("- a\n  - %%%\n{closer}\n    hidden\ntail\n")),
            "<ul><li>a<ul><li>hiddentail</li></ul></li></ul>",
            "{closer:?}"
        );
    }
    assert_eq!(
        flat("- a\n  - %%%`\n%%%\n    hidden\ntail\n"),
        "<ul><li>a<ul><li>hiddentail</li></ul></li></ul>"
    );
}

#[test]
fn a_blank_inside_a_span_cut_by_a_missed_line_loosens_the_item() {
    assert_eq!(
        flat("- a\n  - b\n    %%%\n\n  x\n"),
        "<ul><li><p>a</p><ul><li>b</li></ul><p>x</p></li></ul>"
    );
}

#[test]
fn the_trace_closes_the_span_at_its_closer() {
    let trace = carve::decision_trace("- a\n  - %%%\n%%%\n    hidden\ntail\n");
    assert!(
        trace.contains(r#"{"line":3,"step":"S1","owner":"doc/item:1/item:2","rule":"CARVE-P0-005","form":"closer","cell":"B-comment-end"}"#),
        "{trace}"
    );
}
