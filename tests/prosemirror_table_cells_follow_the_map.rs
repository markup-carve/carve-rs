//! Table cells take the shape the schema map gives them (`types.table_cell`),
//! the one carve-grammars' `carveToProseMirror` writes (markup-carve/carve-rs#4292).

use carve::{from_prosemirror, parse, render_carve, render_html, to_prosemirror};
use serde_json::{json, Value};

fn table(source: &str) -> Value {
    let value: Value = serde_json::from_str(&to_prosemirror(&parse(source)).json).unwrap();
    value["content"][0].clone()
}

#[test]
fn the_ticket_reproduction_follows_the_map() {
    let t = table("|= A |={>} B |\n| x | 1 |\n");
    assert_eq!(
        t["content"][0]["content"][1],
        json!({"type": "tableHeader", "attrs": {"textAlign": "right"},
               "content": [{"type": "paragraph", "content": [{"type": "text", "text": "B"}]}]})
    );
    assert_eq!(
        t["content"][1]["content"][1],
        json!({"type": "tableCell", "attrs": {"carveInheritedTextAlign": "right"},
               "content": [{"type": "paragraph", "content": [{"type": "text", "text": "1"}]}]})
    );
    assert_eq!(
        t["content"][1]["content"][0],
        json!({"type": "tableCell",
               "content": [{"type": "paragraph", "content": [{"type": "text", "text": "x"}]}]})
    );
}

#[test]
fn span_markers_merge_into_their_origin() {
    let t = table("| a | < |\n| b | c |\n| ^ | d |\n");
    let cells = |row: usize| t["content"][row]["content"].clone();
    assert_eq!(cells(0).as_array().unwrap().len(), 1);
    assert_eq!(cells(0)[0]["attrs"], json!({"colspan": 2}));
    assert_eq!(cells(1)[0]["attrs"], json!({"rowspan": 2}));
    assert_eq!(cells(2).as_array().unwrap().len(), 1);
    assert_eq!(cells(2)[0]["content"][0]["content"][0]["text"], json!("d"));
}

#[test]
fn a_covered_column_written_with_a_caret_keeps_its_marker() {
    let source = "| a | < |\n| ^ | ^ |\n| x | y |\n";
    let t = table(source);
    assert_eq!(
        t["content"][0]["content"][0]["attrs"],
        json!({"colspan": 2, "rowspan": 2, "carveCoverMarkers": ["^"]})
    );
    let back = from_prosemirror(&to_prosemirror(&parse(source)).json).unwrap();
    assert_eq!(
        render_carve(&back).unwrap(),
        render_carve(&parse(source)).unwrap()
    );
}

/// Every marker a rowspan swallows is checked, not only the one under the
/// origin's first column.
#[test]
fn a_covered_marker_with_its_own_alignment_is_reported() {
    let pm = to_prosemirror(&parse("| a | < |\n| ^ |{>} < |\n"));
    assert!(pm.degraded.contains_key("table_cell"), "{:?}", pm.degraded);
    let pm = to_prosemirror(&parse("| a | < |\n| ^ | < |\n"));
    assert!(pm.degraded.is_empty(), "{:?}", pm.degraded);
}

#[test]
fn a_blocked_marker_stays_a_cell() {
    let t = table("| ^ | a |\n");
    assert_eq!(
        t["content"][0]["content"][0],
        json!({"type": "tableCell", "attrs": {"carveSpanMarker": "^"},
               "content": [{"type": "paragraph"}]})
    );
}

/// What carve-grammars writes for a spanned, aligned table reads back to the
/// same document.
#[test]
fn the_editor_shape_reads_back() {
    let source = "|= a |={>} b | < |\n| ^ | c | d |\n";
    let stored = json!({"type": "doc", "content": [{"type": "table", "content": [
        {"type": "tableRow", "content": [
            {"type": "tableHeader", "attrs": {"rowspan": 2},
             "content": [{"type": "paragraph", "content": [{"type": "text", "text": "a"}]}]},
            {"type": "tableHeader", "attrs": {"textAlign": "right", "colspan": 2},
             "content": [{"type": "paragraph", "content": [{"type": "text", "text": "b"}]}]}]},
        {"type": "tableRow", "content": [
            {"type": "tableCell", "attrs": {"carveInheritedTextAlign": "right"},
             "content": [{"type": "paragraph", "content": [{"type": "text", "text": "c"}]}]},
            {"type": "tableCell",
             "content": [{"type": "paragraph", "content": [{"type": "text", "text": "d"}]}]}]}]}]});
    let back = from_prosemirror(&stored.to_string()).unwrap();
    assert_eq!(
        render_html(&back).unwrap(),
        render_html(&parse(source)).unwrap()
    );
    assert_eq!(table(source), stored["content"][0]);
}

/// Documents stored before the bridge followed the map still read.
#[test]
fn the_earlier_shape_still_reads() {
    let source = "|= a |={>} b | < |\n| x | y | z |\n";
    let stored = json!({"type": "doc", "content": [{"type": "table", "content": [
        {"type": "tableRow", "content": [
            {"type": "tableHeader", "attrs": {"colspan": 1, "rowspan": 1},
             "content": [{"type": "text", "text": "a"}]},
            {"type": "tableHeader", "attrs": {"alignment": "right", "colspan": 1, "rowspan": 1},
             "content": [{"type": "text", "text": "b"}]},
            {"type": "tableCell", "attrs": {"colspan": 1, "rowspan": 1, "carveSpanMarker": "<"}}]},
        {"type": "tableRow", "content": [
            {"type": "tableCell", "attrs": {"colspan": 1, "rowspan": 1},
             "content": [{"type": "text", "text": "x"}]},
            {"type": "tableCell", "attrs": {"colspan": 1, "rowspan": 1},
             "content": [{"type": "text", "text": "y"}]},
            {"type": "tableCell", "attrs": {"colspan": 1, "rowspan": 1},
             "content": [{"type": "text", "text": "z"}]}]}]}]});
    let back = from_prosemirror(&stored.to_string()).unwrap();
    assert_eq!(
        render_html(&back).unwrap(),
        render_html(&parse(source)).unwrap()
    );
}

/// Generated tables (spans, blocked markers, header rows, separator rows,
/// alignments) survive the round trip unless the bridge reports a loss.
#[test]
fn generated_tables_round_trip_or_report() {
    let mut seed: u64 = 4292;
    let mut next = |n: u64| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) % n
    };
    let aligns = ["", "", "{<}", "{>}", "{~}", "{^}", "{v}", "{>v}"];
    let texts = ["x", "yy", "*b*", "1", "`c`"];
    let (mut strict, mut reported) = (0, 0);
    for _ in 0..1200 {
        let cols = 2 + next(3);
        let rows = 1 + next(4);
        let separator = next(5) == 0;
        let mut lines = Vec::new();
        for r in 0..rows {
            let mut cells = Vec::new();
            for c in 0..cols {
                let header = !separator && if r == 0 { next(4) != 0 } else { next(8) == 0 };
                let k = next(10);
                let body = if k < 2 && c > 0 {
                    "<"
                } else if k < 4 && r > 0 {
                    "^"
                } else {
                    texts[next(texts.len() as u64) as usize]
                };
                let align = if next(3) == 0 {
                    aligns[next(aligns.len() as u64) as usize]
                } else {
                    ""
                };
                cells.push(format!("{}{align} {body} ", if header { "=" } else { "" }));
            }
            lines.push(format!("|{}|", cells.join("|")));
            if separator && r == 0 {
                let marks: Vec<_> = (0..cols)
                    .map(|_| [":--", "--:", ":-:", "---"][next(4) as usize])
                    .collect();
                lines.push(format!("|{}|", marks.join("|")));
            }
        }
        let source = lines.join("\n") + "\n";
        let original = parse(&source);
        let pm = to_prosemirror(&original);
        if !pm.dropped.is_empty() || !pm.degraded.is_empty() {
            // The only loss these shapes have: a marker with its own alignment
            // or header flag merged into its origin's span.
            assert!(pm.dropped.is_empty(), "{source}\n{:?}", pm.dropped);
            assert_eq!(
                pm.degraded.keys().collect::<Vec<_>>(),
                ["table_cell"],
                "{source}"
            );
            reported += 1;
            continue;
        }
        let back = from_prosemirror(&pm.json).unwrap();
        assert_eq!(
            render_html(&back).unwrap(),
            render_html(&original).unwrap(),
            "{source}\n{}",
            pm.json
        );
        strict += 1;
    }
    assert!(
        strict > 500 && reported > 300,
        "{strict} strict, {reported} reported"
    );
}
