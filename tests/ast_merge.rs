use carve::{merge_ast, parse, to_json, MergeConflictReason, MergeResult};

#[test]
fn merges_independent_node_edits() {
    let base = parse("# Heading\n\nBody.\n");
    let ours = parse("# Ours\n\nBody.\n");
    let theirs = parse("# Heading\n\nTheirs.\n");
    let MergeResult::Merged(merged) = merge_ast(&base, &ours, &theirs).unwrap() else {
        panic!("unexpected conflict")
    };
    let json = to_json(&merged);
    assert!(json.contains("Ours"));
    assert!(json.contains("Theirs"));
}

#[test]
fn reports_same_leaf_edits() {
    let base = parse("Body.\n");
    let ours = parse("Ours.\n");
    let theirs = parse("Theirs.\n");
    let MergeResult::Conflicts(conflicts) = merge_ast(&base, &ours, &theirs).unwrap() else {
        panic!("expected conflict")
    };
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].reason, MergeConflictReason::BothChanged);
    assert!(conflicts[0].path.starts_with("/children/0"));
}

#[test]
fn combines_insertions_at_distinct_anchors() {
    let base = parse("A.\n\nB.\n");
    let ours = parse("Before.\n\nA.\n\nB.\n");
    let theirs = parse("A.\n\nB.\n\nAfter.\n");
    let MergeResult::Merged(merged) = merge_ast(&base, &ours, &theirs).unwrap() else {
        panic!("unexpected conflict")
    };
    let json = to_json(&merged);
    assert!(json.find("Before").unwrap() < json.find("A.").unwrap());
    assert!(json.find("B.").unwrap() < json.find("After").unwrap());
}

#[test]
fn rejects_incompatible_reorders() {
    let base = parse("A.\n\nB.\n\nC.\n");
    let ours = parse("B.\n\nA.\n\nC.\n");
    let theirs = parse("A.\n\nC.\n\nB.\n");
    let MergeResult::Conflicts(conflicts) = merge_ast(&base, &ours, &theirs).unwrap() else {
        panic!("expected conflict")
    };
    assert!(conflicts
        .iter()
        .any(|item| item.reason == MergeConflictReason::ConcurrentSequenceEdit));
}

#[test]
fn preserves_attributes_named_like_position_metadata() {
    let base = parse("[text]{pos=base srcByteLength=kept}\n");
    let ours = parse("[text]{pos=ours srcByteLength=kept}\n");
    let theirs = parse("[changed]{pos=base srcByteLength=kept}\n");
    let MergeResult::Merged(merged) = merge_ast(&base, &ours, &theirs).unwrap() else {
        panic!("unexpected conflict")
    };
    let json = to_json(&merged);
    assert!(json.contains("\"pos\":\"ours\""));
    assert!(json.contains("\"srcByteLength\":\"kept\""));
    assert!(!json.contains("\"startLine\""));
}

#[test]
fn uses_the_empty_pointer_for_a_root_conflict() {
    let base = parse("Base.\n");
    let ours = parse("Ours.\n");
    let theirs = parse("Theirs.\n");
    let MergeResult::Conflicts(conflicts) = merge_ast(&base, &ours, &theirs).unwrap() else {
        panic!("expected conflict")
    };
    assert!(conflicts.iter().all(|conflict| conflict.path != "/"));
}
