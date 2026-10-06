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

#[test]
fn batch_edits_preserve_insertion_order_and_untouched_identity() {
    let first = carve::parse_snapshot_with_identity("one\n\ntwo\n\nthree\n\nfour");
    let previous = first.node_identity.as_ref().unwrap();
    let middle = previous
        .nodes
        .iter()
        .find(|node| node.path == "/children/1")
        .unwrap()
        .id
        .clone();
    let second = reparse(
        first.snapshot,
        &[
            TextChange {
                range: 0..0,
                replacement: "A".into(),
            },
            TextChange {
                range: 0..0,
                replacement: "B".into(),
            },
            TextChange {
                range: 0..3,
                replacement: "ONE".into(),
            },
            TextChange {
                range: 17..21,
                replacement: "4".into(),
            },
        ],
    )
    .unwrap();
    assert_eq!(second.snapshot.source(), "ABONE\n\ntwo\n\nthree\n\n4");
    assert_eq!(
        second
            .node_identity
            .unwrap()
            .nodes
            .iter()
            .find(|node| node.path == "/children/1")
            .unwrap()
            .id,
        middle
    );
}

#[test]
fn invalid_batches_keep_the_previous_error_precedence() {
    let result = reparse(
        parse_snapshot("éx").snapshot,
        &[
            TextChange {
                range: 1..2,
                replacement: String::new(),
            },
            TextChange {
                range: 9..10,
                replacement: String::new(),
            },
        ],
    );
    assert_eq!(
        result.unwrap_err().to_string(),
        "text change is out of bounds"
    );
}
