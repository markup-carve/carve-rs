use carve::{
    merge_ast, merge_ast_with_resolver, parse, to_json, MergeConflictReason, MergeResolution,
    MergeResult,
};

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

#[test]
fn resolves_a_conflict_from_the_application() {
    let base = parse("# Base\n");
    let ours = parse("# Ours\n");
    let theirs = parse("# Theirs\n");
    let MergeResult::Merged(merged) =
        merge_ast_with_resolver(&base, &ours, &theirs, |_| Some(MergeResolution::Ours)).unwrap()
    else {
        panic!("unexpected conflict")
    };
    assert!(to_json(&merged).contains("Ours"));
}

#[test]
fn reports_delete_against_edit() {
    let base = parse("alpha\n\nbeta\n");
    let ours = parse("alpha\n");
    let theirs = parse("alpha\n\nbeta edited\n");
    let MergeResult::Conflicts(conflicts) = merge_ast(&base, &ours, &theirs).unwrap() else {
        panic!("expected conflict")
    };
    let conflict = conflicts
        .iter()
        .find(|conflict| conflict.reason == MergeConflictReason::DeleteEdit)
        .unwrap();
    assert!(conflict.ours.is_none());
}

#[test]
fn deduplicates_the_same_concurrent_insertion() {
    let base = parse("one\n");
    let ours = parse("one\n\ntwo\n");
    let theirs = parse("one\n\ntwo\n");
    let MergeResult::Merged(merged) = merge_ast(&base, &ours, &theirs).unwrap() else {
        panic!("unexpected conflict")
    };
    assert_eq!(to_json(&merged).matches("\"two\"").count(), 1);
}

#[test]
fn merges_an_edit_into_a_node_moved_by_the_other_side() {
    let base = parse("alpha\n\nbeta\n");
    let ours = parse("beta\n\nalpha\n");
    let theirs = parse("alpha\n\nbeta edited\n");
    let MergeResult::Merged(merged) = merge_ast(&base, &ours, &theirs).unwrap() else {
        panic!("unexpected conflict")
    };
    let json = to_json(&merged);
    assert!(json.find("beta edited").unwrap() < json.find("alpha").unwrap());
}

#[test]
fn conflicts_on_different_definitions_with_the_same_identity() {
    let base = parse("Body.\n");
    let ours = parse("Body.\n\n[^same]: ours\n");
    let theirs = parse("Body.\n\n[^same]: theirs\n");
    let MergeResult::Conflicts(conflicts) = merge_ast(&base, &ours, &theirs).unwrap() else {
        panic!("expected conflict")
    };
    assert_eq!(
        conflicts[0].reason,
        MergeConflictReason::ConcurrentSequenceEdit
    );
}

#[test]
fn bounds_a_wide_ambiguous_sibling_list() {
    let source = |prefix: &str| {
        (0..1001)
            .map(|index| format!("{prefix}{index}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let started = std::time::Instant::now();
    let result = merge_ast(
        &parse(&source("base-")),
        &parse(&source("ours-")),
        &parse(&source("theirs-")),
    )
    .unwrap();
    assert!(matches!(result, MergeResult::Conflicts(_)));
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}
