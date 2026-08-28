//! A number in a patch value keeps the form it was written in.
//!
//! The value is re-serialized from what arrived rather than re-derived, so an
//! authored `1.0` stays a float. The hand-rolled writer this replaces went
//! through `f64::to_string`, which prints `1.0` as `1` and so silently
//! rewrote an authored float as an integer on the way back out.
//!
//! `table.columns[].width` is the field this can reach: it is a float bounded
//! to `(0, 1]`, so `1.0` is a legal value sitting exactly on the boundary.

use carve::{ast_patch_from_json, AstPatchOperation};

fn added_value(patch: &str) -> String {
    let operations = ast_patch_from_json(patch).expect("patch parses");
    match operations.into_iter().next().expect("one operation") {
        AstPatchOperation::Add { value, .. } => value,
        other => panic!("expected an add, got {other:?}"),
    }
}

#[test]
fn a_whole_float_stays_a_float() {
    let patch = r#"[{"op":"add","path":"/children/0/width","value":1.0}]"#;
    assert_eq!(added_value(patch), "1.0");
}

#[test]
fn an_integer_stays_an_integer() {
    let patch = r#"[{"op":"add","path":"/children/0/width","value":1}]"#;
    assert_eq!(added_value(patch), "1");
}

#[test]
fn a_fractional_width_is_unchanged() {
    let patch = r#"[{"op":"add","path":"/children/0/width","value":0.5}]"#;
    assert_eq!(added_value(patch), "0.5");
}
