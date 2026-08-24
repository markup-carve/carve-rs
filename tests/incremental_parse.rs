use carve::{parse_snapshot, parse_with_source_layout, reparse, to_json, TextChange};

#[test]
fn a_snapshot_accepts_non_overlapping_utf8_byte_edits() {
    let first = parse_snapshot("# Title\n\nBody.\n");
    let second = reparse(
        first.snapshot,
        &[TextChange {
            range: 9..13,
            replacement: "Text".into(),
        }],
    )
    .expect("valid edit");
    assert_eq!(second.snapshot.source(), "# Title\n\nText.\n");
    assert_eq!(second.changed_source, vec![9..13]);
    let (fresh, _) = parse_with_source_layout("# Title\n\nText.\n");
    assert_eq!(to_json(&second.document), to_json(&fresh));
}

#[test]
fn overlapping_and_non_boundary_edits_are_rejected() {
    let snapshot = parse_snapshot("éx").snapshot;
    assert!(reparse(
        snapshot.clone(),
        &[TextChange {
            range: 1..2,
            replacement: String::new(),
        }]
    )
    .is_err());
    assert!(reparse(
        snapshot,
        &[
            TextChange {
                range: 0..1,
                replacement: String::new(),
            },
            TextChange {
                range: 0..2,
                replacement: String::new(),
            },
        ]
    )
    .is_err());
}
