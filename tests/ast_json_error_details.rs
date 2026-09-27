use carve::{from_json, AstJsonErrorKind};
use std::error::Error;

#[test]
fn syntax_errors_keep_their_cause_and_location() {
    let error = from_json("{\n  !\n}").unwrap_err();
    assert_eq!(error.kind(), AstJsonErrorKind::Syntax);
    assert_eq!(error.line(), Some(2));
    assert!(error.column().unwrap() > 0);
    assert!(error.path().is_none());
    let cause = error.source().unwrap();
    assert!(cause.downcast_ref::<serde_json::Error>().is_some());
    assert_eq!(error.to_string(), cause.to_string());
    assert_eq!(error, error.clone());
}

#[test]
fn depth_refusal_is_distinct_from_invalid_syntax() {
    let error = from_json(&"[".repeat(10_000)).unwrap_err();
    assert_eq!(error.kind(), AstJsonErrorKind::DepthLimit);
    assert!(error.source().is_none());
    assert!(error.line().is_none());
}

#[test]
fn unknown_fields_identify_the_nested_location() {
    let error = from_json(r#"{"type":"document","srcByteLength":0,"children":[{"type":"paragraph","children":[],"extra":true}]}"#).unwrap_err();
    assert_eq!(error.kind(), AstJsonErrorKind::UnknownField);
    assert_eq!(error.path(), Some("children[0].extra"));
    assert!(error.source().is_none());
    assert!(error.to_string().contains("carries \"extra\""));
}

#[test]
fn invalid_ast_is_distinct_from_json_syntax() {
    let error = from_json(r#"{"type":"wrong"}"#).unwrap_err();
    assert_eq!(error.kind(), AstJsonErrorKind::InvalidAst);
}

#[test]
fn root_field_paths_preserve_leading_dots_in_the_name() {
    let error = from_json(r#"{"type":"document","srcByteLength":0,"children":[],".extra":true}"#)
        .unwrap_err();
    assert_eq!(error.kind(), AstJsonErrorKind::UnknownField);
    assert_eq!(error.path(), Some(".extra"));
}
