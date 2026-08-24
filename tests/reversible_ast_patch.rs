use carve::{apply_reversible_ast_patch, create_reversible_ast_patch, parse, to_json};

#[test]
fn forward_and_inverse_restore_the_documents() {
    let before = parse("# Before\n\nText.\n");
    let after = parse("# After\n\nChanged.\n");
    let patch = create_reversible_ast_patch(&before, &after).expect("patch builds");
    let applied = apply_reversible_ast_patch(&before, &patch, false).expect("forward applies");
    let mut expected_after = after.clone();
    expected_after.source_len = 0;
    assert_eq!(to_json(&applied), to_json(&expected_after));
    let restored = apply_reversible_ast_patch(&applied, &patch, true).expect("inverse applies");
    let mut expected_before = before.clone();
    expected_before.source_len = 0;
    assert_eq!(to_json(&restored), to_json(&expected_before));
}

#[test]
fn a_patch_rejects_the_wrong_document() {
    let before = parse("Before.\n");
    let after = parse("After.\n");
    let unrelated = parse("Unrelated.\n");
    let patch = create_reversible_ast_patch(&before, &after).expect("patch builds");
    let error = apply_reversible_ast_patch(&unrelated, &patch, false)
        .expect_err("stale precondition is rejected");
    assert!(error.to_string().contains("precondition"));
}
