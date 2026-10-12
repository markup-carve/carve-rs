use carve::{parse_with_options, render_html, Options};

#[test]
fn continued_marker_notes_preserve_code_ownership() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/continued_marker_note_code_floor.json"
    ))
    .unwrap();
    for row in cases.as_array().unwrap() {
        for positions in [false, true] {
            let doc = parse_with_options(
                row["source"].as_str().unwrap(),
                &Options::default().with_positions(positions),
            );
            assert_eq!(
                render_html(&doc).unwrap().trim(),
                row["html"].as_str().unwrap().trim(),
                "{} positions={positions}",
                row["id"]
            );
            if row["id"].as_str().unwrap().contains("col4") {
                assert!(doc.footnote_defs["n"]
                    .iter()
                    .all(|block| matches!(block, carve::ast::BlockNode::Paragraph(_))));
                assert!(!render_html(&doc).unwrap().contains('\u{e000}'));
            }
        }
    }
}
