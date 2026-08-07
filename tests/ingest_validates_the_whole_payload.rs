//! PART 12 §12(d): AN INGEST VALIDATES THE WHOLE PAYLOAD (carve#881).
//!
//! > (d) An ingest validates the WHOLE payload against
//! > `resources/ast-schema.json` - types and required fields together, at
//! > DECODE, refused with the same typed error §12(a), (b) and (c) already
//! > require.
//!
//! Not a fourth list of leniency points. The schema is the list, it already
//! describes every row that diverged, and those rows were only ever divergent
//! because nothing consulted it.
//!
//! THIS ENGINE ALREADY REFUSES ALL SIXTEEN, which is what the divergence table
//! on carve#881 records for it: `decode` in every row, while carve-js rendered
//! or accepted eleven of them and carve-php accepted nine and failed two more
//! with an UNTYPED `TypeError`. So this file pins rather than fixes. The rows
//! were pinned individually in `root_shape_is_refused_on_ingest` and
//! `unknown_wire_property_is_refused` at best, and eight of them nowhere - a
//! behavior that is right and unpinned is one edit from being wrong and quiet,
//! and this is a clause whose whole point is that future schema additions
//! become rejections.
//!
//! THE BASE DOCUMENT IS ASSERTED TO BE ACCEPTED, and that assertion is not
//! ceremony: without it, sixteen rejections of a document that was never valid
//! would read exactly like a clause being enforced. Same shape as the opt-in
//! trap in `tests/autolink_url_char_classes.rs` (carve#755).
//!
//! WHAT §12(d) DOES NOT REACH is pinned too: a `srcByteLength` that is PRESENT
//! but WRONG stays accepted. It is derivable and nothing in the tree depends on
//! it - (a) is about the field's presence, (d) about its type and sign, and
//! neither is about the number being right. Pinned so that tightening cannot
//! quietly annex it.

use carve::from_json;

const POS: &str =
    r#"{"startLine":1,"endLine":1,"startColumn":1,"endColumn":2,"startOffset":0,"endOffset":1}"#;

/// The valid document all sixteen refusals are built from.
fn base() -> String {
    format!(
        r#"{{"type":"document","srcByteLength":2,"children":[{{"type":"paragraph","pos":{POS},"children":[{{"type":"text","value":"x","pos":{POS}}}]}}]}}"#
    )
}

fn accepted(what: &str, doc: &str) {
    assert!(
        from_json(doc).is_ok(),
        "§12(d) refused {what}, which it must not: {doc}"
    );
}

// ---------------------------------------------------------------------------
// The control the sixteen are built from
// ---------------------------------------------------------------------------

#[test]
fn the_payload_the_sixteen_are_built_from_is_itself_accepted() {
    accepted("the base document", &base());
}

// ---------------------------------------------------------------------------
// The sixteen, as a table walked by one test
// ---------------------------------------------------------------------------

/// Every payload §12(d) refuses, each built from the valid one above. A TABLE
/// rather than sixteen test functions, so the number of rows EXAMINED can be
/// asserted against the clause's own count: zero findings from zero rows reads
/// exactly like a clean run, and a table that silently lost a row would report
/// one.
fn refused_rows() -> Vec<(&'static str, String)> {
    let inline = format!(r#"[{{"type":"text","value":"x","pos":{POS}}}]"#);
    vec![
        (
            "a root srcByteLength of the wrong type",
            base().replace(r#""srcByteLength":2"#, r#""srcByteLength":"2""#),
        ),
        (
            "a negative root srcByteLength",
            base().replace(r#""srcByteLength":2"#, r#""srcByteLength":-1"#),
        ),
        (
            "root children of the wrong type",
            r#"{"type":"document","srcByteLength":2,"children":"x"}"#.to_string(),
        ),
        // §12's own objection arriving through a door the clause did not cover:
        // a reader that supplies a default has turned a truncated document into
        // an empty one.
        (
            "root children of null",
            r#"{"type":"document","srcByteLength":2,"children":null}"#.to_string(),
        ),
        (
            "a node missing type",
            base().replacen(r#""type":"paragraph","#, "", 1),
        ),
        (
            "a node type that is not a string",
            base().replacen(r#""type":"paragraph""#, r#""type":7"#, 1),
        ),
        (
            "a paragraph missing children",
            format!(
                r#"{{"type":"document","srcByteLength":2,"children":[{{"type":"paragraph","pos":{POS}}}]}}"#
            ),
        ),
        (
            "a text node missing value",
            base().replacen(r#""value":"x","#, "", 1),
        ),
        // The defect the clause was written to close: one engine rendered
        // `<p>7</p>`.
        (
            "a text value that is a number",
            base().replacen(r#""value":"x""#, r#""value":7"#, 1),
        ),
        (
            "a child that is null",
            base().replacen(&inline, "[null]", 1),
        ),
        (
            "a child that is a string",
            base().replacen(&inline, r#"["x"]"#, 1),
        ),
        // The mistake a producer will actually make, since `class` is what the
        // rendered HTML calls the thing - and one engine ACCEPTED it and
        // rendered `class="x"`, so a producer testing against that engine ships
        // something the other two reject outright.
        (
            "attrs spelled class",
            base().replacen(
                r#""type":"paragraph","#,
                r#""type":"paragraph","attrs":{"class":"x"},"#,
                1,
            ),
        ),
        (
            "attrs carrying an unnamed key beside keyValues",
            base().replacen(
                r#""type":"paragraph","#,
                r#""type":"paragraph","attrs":{"keyValues":{"a":"b"},"bogus":1},"#,
                1,
            ),
        ),
        (
            "attrs of the wrong type",
            base().replacen(
                r#""type":"paragraph","#,
                r#""type":"paragraph","attrs":"x","#,
                1,
            ),
        ),
        (
            "a pos carrying an extra key",
            base().replacen(r#""endOffset":1}"#, r#""endOffset":1,"extra":1}"#, 1),
        ),
        (
            "a pos missing endOffset",
            base().replacen(r#","endOffset":1}"#, "}", 1),
        ),
    ]
}

#[test]
fn the_sixteen_payloads_are_refused_at_decode() {
    let rows = refused_rows();
    let accepted: Vec<&str> = rows
        .iter()
        .filter(|(_, doc)| from_json(doc).is_ok())
        .map(|(what, _)| *what)
        .collect();
    assert!(
        accepted.is_empty(),
        "§12(d) accepted these, so it refuses nothing for them: {accepted:?}"
    );
}

#[test]
fn every_refusal_carries_the_typed_error_section_12_requires() {
    // Two engines failed rows of this table with an UNTYPED error, which §9(b)
    // already forbids: a bare PHP `TypeError` from the codec, and `nodes is not
    // iterable` from a RENDERER - i.e. after the decode that should have
    // refused it. Refusing is only half the clause.
    for (what, doc) in refused_rows() {
        let err = from_json(&doc).expect_err(what);
        assert!(
            !err.to_string().trim().is_empty(),
            "{what}: the refusal carries no message"
        );
    }
}

#[test]
fn every_row_the_clause_names_is_examined_here() {
    // The DENOMINATOR, asserted against the clause's own count of the divergence
    // table on carve#881. Sixteen rows walked twice by the two tests above, plus
    // the base-document control, without which sixteen rejections of a document
    // that was never valid would read exactly like a clause being enforced.
    assert_eq!(
        refused_rows().len(),
        16,
        "the payload table lost or gained rows"
    );
    let mut names: Vec<&str> = refused_rows().iter().map(|(what, _)| *what).collect();
    names.sort_unstable();
    let unique = {
        let mut n = names.clone();
        n.dedup();
        n.len()
    };
    assert_eq!(
        unique, 16,
        "a row is listed twice, so one tests nothing new"
    );
}

// ---------------------------------------------------------------------------
// What (d) does NOT reach
// ---------------------------------------------------------------------------

#[test]
fn control_a_src_byte_length_that_is_merely_wrong_stays_accepted() {
    accepted(
        "a srcByteLength that is present, well-typed and wrong",
        &base().replace(r#""srcByteLength":2"#, r#""srcByteLength":999"#),
    );
}

#[test]
fn control_this_engine_still_reads_its_own_output() {
    // The other direction of the same clause: a validating ingest that refuses
    // the engine's own trees would satisfy every rejection above and be useless.
    let source = "# T\n\nx [a](/u) `c`\n\n- i\n\n| a |\n";
    let json = carve::to_json_with_options(source, &{
        let mut o = carve::Options::new();
        o.positions = true;
        o
    });
    assert!(
        json.contains("\"pos\""),
        "the probe did not request positions"
    );
    accepted("this engine's own serialized tree", &json);
}
