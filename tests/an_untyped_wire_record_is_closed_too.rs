//! PART 12 S11 refuses a property the schema does not name - and the check that
//! enforces it is keyed by a node's `type`, so every wire record that has no
//! `type` of its own escaped it entirely.
//!
//! Three sites were found by sweeping rather than by working the list on the
//! ticket, which named one (carve-rs#820):
//!
//! 1. the `footnote.id` ALIAS, an unnamed property this decoder deliberately
//!    read - covered next door in `unknown_wire_property_is_refused.rs`;
//! 2. the LEGACY definition entry, `{terms, definitions}`, an object the schema
//!    gives no `type` because the current wire form is a flat run of
//!    `definition_term` and `definition_description` nodes. carve-js found this
//!    one on its half of the same ticket (carve-js#913) and this engine had it
//!    too;
//! 3. the CITATION node inside `citation_group.items`, which began as an
//!    untyped record and is now a typed, positioned node. It remains covered
//!    here because closing the record against extra fields is still required.
//!
//! Every case pairs the bogus field with a CONTROL carrying only named fields,
//! so none of them can pass because decoding was refused wholesale.

use carve::from_json;

const DOC: &str = r#"{"type":"document","srcByteLength":9,"children":[NODES]}"#;

fn decode(nodes: &str) -> Result<carve::Document, carve::AstJsonError> {
    from_json(&DOC.replace("NODES", nodes))
}

fn refusal(nodes: &str) -> String {
    decode(nodes)
        .err()
        .map(|error| error.to_string())
        .unwrap_or_else(|| panic!("accepted: {nodes}"))
}

// ---------------------------------------------------------------------------
// 2. The legacy definition entry.
// ---------------------------------------------------------------------------

const LEGACY_ENTRY: &str = r#"{"type":"definition_list","items":[{"terms":[[{"type":"text","value":"t"}]],"definitions":[[]]FIELDS}]}"#;

#[test]
fn a_legacy_definition_entry_refuses_a_field_the_schema_does_not_name() {
    let error = refusal(&LEGACY_ENTRY.replace("FIELDS", r#","bogus":1"#));
    assert!(error.contains("\"bogus\""), "{error}");
    assert!(error.contains("PART 12"), "{error}");
}

#[test]
fn control_a_legacy_definition_entry_still_decodes() {
    // The form is still READ - trees in it are stored. Only the fields on it
    // are closed.
    let doc = decode(&LEGACY_ENTRY.replace("FIELDS", "")).expect("the legacy form still decodes");
    let html = carve::render_html(&doc).expect("render");
    assert!(html.contains("<dt>t</dt>"), "{html}");
}

#[test]
fn control_a_legacy_definition_entry_keeps_the_publishers_position_arrays() {
    // The allowed set is the one carve-js closed the same record to, including
    // the two position arrays its runtime carries: the legacy publisher WAS
    // that runtime, and a narrower set here would refuse a stored payload
    // carve-js accepts - the interchange break S11 exists to prevent, not to
    // cause. This engine drops the arrays, as it drops every position on that
    // path.
    assert!(decode(&LEGACY_ENTRY.replace(
        "FIELDS",
        r#","definitionLines":[1],"definitionSpans":[[0,1]]"#
    ))
    .is_ok());
}

#[test]
fn a_typed_node_carrying_terms_is_refused_under_its_own_type() {
    // THE `type`-ABSENT GUARD IS ABOUT THE MESSAGE, and this is what makes it
    // load-bearing rather than decorative. Removing the guard kills no other
    // case here - no node the schema names carries an array-valued `terms`, so
    // a typed node carrying one is refused either way. What changes is WHICH
    // rule refuses it: without the guard the payload comes back as a legacy
    // definition entry carrying `"type"`, which sends the caller after a record
    // their document does not contain.
    let error = refusal(r#"{"type":"paragraph","terms":[],"children":[]}"#);
    assert!(error.contains("paragraph"), "{error}");
    assert!(error.contains("\"terms\""), "{error}");
    assert!(
        !error.contains("legacy definition entry"),
        "a typed node is not a legacy entry: {error}"
    );
}

#[test]
fn control_an_attribute_literally_named_terms_is_not_a_legacy_entry() {
    // The false positive the array-valued test exists to stop.
    // `attrs.keyValues` is an OPEN map of strings, so a document with an
    // attribute named `terms` must not be read as a legacy entry and have its
    // other attributes refused.
    assert!(decode(
        r#"{"type":"paragraph","attrs":{"keyValues":{"terms":"x","other":"y"}},"children":[]}"#
    )
    .is_ok());
}

// ---------------------------------------------------------------------------
// 3. The citation record.
// ---------------------------------------------------------------------------

const CITATION: &str = r#"{"type":"paragraph","children":[{"type":"citation_group","raw":"[@a]","items":[{"type":"citation","key":"a","suppressAuthor":false,"pos":{"startLine":1,"endLine":1,"startColumn":1,"endColumn":3,"startOffset":0,"endOffset":2}FIELDS}]}]}"#;

#[test]
fn a_citation_record_refuses_a_field_the_schema_does_not_name() {
    let error = refusal(&CITATION.replace("FIELDS", r#","bogus":1"#));
    assert!(error.contains("\"bogus\""), "{error}");
    assert!(
        error.contains("citation at") && error.contains("items[0]"),
        "{error}"
    );
}

#[test]
fn control_a_citation_record_still_decodes() {
    assert!(decode(&CITATION.replace("FIELDS", "")).is_ok());
}

#[test]
fn control_every_field_the_schema_names_on_a_citation_still_decodes() {
    // The check reads its allowed set from the generated table, so a stale
    // table would show up here as a refusal of a legitimate field rather than
    // as a missing refusal above.
    assert!(decode(&CITATION.replace(
        "FIELDS",
        r#","locatorLabel":"page","locatorValue":"33","number":1,"useIndex":0,"prefix":[],"suffix":[],"locator":[]"#
    ))
    .is_ok());
}

// ---------------------------------------------------------------------------
// The dead spellings. Each of these WAS a fallback in the decoder, and each was
// unreachable, because the unknown-field check refuses the alias before the
// decoder can consult it. They are gone; the tests stay, so that removing the
// check would show up as an accepted payload rather than as nothing.
// ---------------------------------------------------------------------------

#[test]
fn a_code_block_title_alias_is_refused() {
    let error = refusal(r#"{"type":"code_block","title":"t","content":"x"}"#);
    assert!(error.contains("\"title\""), "{error}");
    assert!(decode(r#"{"type":"code_block","header":"t","content":"x"}"#).is_ok());
}

#[test]
fn an_inline_extension_children_alias_is_refused() {
    let error = refusal(
        r#"{"type":"paragraph","children":[{"type":"inline_extension","name":"x","children":[]}]}"#,
    );
    assert!(error.contains("\"children\""), "{error}");
    assert!(decode(
        r#"{"type":"paragraph","children":[{"type":"inline_extension","name":"x","content":[]}]}"#
    )
    .is_ok());
}
