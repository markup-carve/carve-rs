//! A parse records `delim` only for `)` and `bullet_char` only for `*`.
//!
//! `resources/ast-schema.json` calls `delim: "."` and `bulletChar: "-"` absent,
//! which binds a consumer reading a tree and left the parser free to record
//! them anyway. It did, so one document had two legal trees and a reader
//! comparing AST JSON across engines, or against a tree an earlier version
//! stored, had to know which it held. `taskState` already settles the same
//! question the other way (markup-carve/carve#2828).
//!
//! The non-default cases are asserted beside the absences on purpose: "omit the
//! default" and "drop the field" are one line apart in the parser and only one
//! of them is the ruling.

/// The whole tree as JSON. Every document below holds exactly one list, and
/// the key is matched with its colon: `"delimited"` on a comment node starts
/// with the same six characters as `"delim"`.
fn list_json(src: &str) -> String {
    carve::to_json(&carve::parse(src))
}

#[test]
fn a_default_ordered_delimiter_is_not_recorded() {
    let head = list_json("1. Note text.\n");
    assert!(!head.contains("\"delim\":"), "{head}");
}

#[test]
fn a_default_bullet_character_is_not_recorded() {
    let head = list_json("- item\n");
    assert!(!head.contains("\"bulletChar\":"), "{head}");
}

#[test]
fn a_non_default_marker_is_still_recorded() {
    assert!(list_json("1) Note text.\n").contains("\"delim\":\")\""));
    assert!(list_json("* item\n").contains("\"bulletChar\":\"*\""));
}

#[test]
fn the_bare_dot_marker_still_says_so() {
    // `. a` carries no number and no delimiter field now, so `bareMarker` is
    // the only thing separating it from `1. a`. Without this the omission would
    // silently take the one authored distinction the field was added for.
    let head = list_json(". a\n");
    assert!(!head.contains("\"delim\":"), "{head}");
    assert!(head.contains("\"bareMarker\":true"), "{head}");
}

#[test]
fn omitting_the_default_loses_nothing() {
    // The asymmetry the ruling turns on: the two spellings describe one
    // document, so the canonical writer reaches the authored source back.
    for src in [
        "1. Note text.\n",
        "1) Note text.\n",
        "- item\n",
        "* item\n",
        ". a\n",
    ] {
        let written =
            carve::render_carve(&carve::parse(src)).expect("the writer spells every marker");
        assert_eq!(written, src, "{src:?} is not a fixed point");
    }
}
