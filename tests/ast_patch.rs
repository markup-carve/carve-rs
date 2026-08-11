use carve::{apply_ast_patch, create_ast_patch, parse, to_json, AstPatchOperation};

#[test]
fn patch_replays_semantic_changes() {
    let before = parse("# Before\n\nBody.\n");
    let after = parse("# After\n\nBody changed.\n\nAdded.\n");
    let patch = create_ast_patch(&before, &after).unwrap();
    let replayed = apply_ast_patch(&before, &patch).unwrap();
    let mut expected = after;
    expected.source_len = 0;
    assert_eq!(to_json(&replayed), to_json(&expected));
}

#[test]
fn refuses_root_removal() {
    let doc = parse("Body.\n");
    let error = apply_ast_patch(
        &doc,
        &[AstPatchOperation::Remove {
            path: String::new(),
        }],
    )
    .unwrap_err();
    assert!(error.to_string().contains("root"));
}

#[test]
fn refuses_an_out_of_range_index() {
    let doc = parse("Body.\n");
    let error = apply_ast_patch(
        &doc,
        &[AstPatchOperation::Remove {
            path: "/children/99".into(),
        }],
    )
    .unwrap_err();
    assert!(error.to_string().contains("out of range"));
}
