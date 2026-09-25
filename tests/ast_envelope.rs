use carve::{
    from_ast_envelope_json, from_json, parse, to_ast_envelope_json, to_json, AstEnvelopeError,
    AstEnvelopeExtension, AstEnvelopeOptions, AstEnvelopeReaderOptions, AST_CONTRACT_VERSION,
    CORE_AST_VOCABULARY,
};

const CITATIONS: &str = "https://markup-carve.org/ext/citations";

fn envelope(doc_json: &str, fields: &[(&str, &str)]) -> String {
    let mut out = String::from("{");
    for (name, value) in fields {
        out.push_str(&format!("\"{name}\":{value},"));
    }
    out.push_str(&format!("\"document\":{doc_json}}}"));
    out
}

fn tree() -> String {
    to_json(&parse("# Title\n\nBody.\n"))
}

#[test]
fn encode_wraps_the_tree_without_moving_it() {
    let doc = parse("# Title\n\nBody.\n");
    let wrapped = to_ast_envelope_json(&doc, &AstEnvelopeOptions::default()).unwrap();
    assert!(wrapped.starts_with("{\"astVersion\":\"1.0\","));
    assert_eq!(AST_CONTRACT_VERSION, "1.0");
    assert!(wrapped.contains(&format!("\"document\":{}", to_json(&doc))));
    assert!(!wrapped.contains("\"vocabulary\""));
    assert!(!wrapped.contains("\"extensions\""));
}

#[test]
fn encode_carries_a_vocabulary_and_extensions_only_when_given() {
    let doc = parse("Body.\n");
    let wrapped = to_ast_envelope_json(
        &doc,
        &AstEnvelopeOptions {
            vocabulary: Some(CORE_AST_VOCABULARY.to_string()),
            extensions: vec![AstEnvelopeExtension {
                id: CITATIONS.to_string(),
                version: Some("1".to_string()),
                required: Some(false),
            }],
        },
    )
    .unwrap();
    assert!(wrapped.contains(&format!("\"vocabulary\":\"{CORE_AST_VOCABULARY}\"")));
    assert!(wrapped.contains(&format!(
        "\"extensions\":[{{\"id\":\"{CITATIONS}\",\"version\":\"1\",\"required\":false}}]"
    )));
}

#[test]
fn round_trip_returns_the_same_tree() {
    let doc = parse("# Title\n\nBody with *emphasis*.\n");
    let wrapped = to_ast_envelope_json(&doc, &AstEnvelopeOptions::default()).unwrap();
    let read = from_ast_envelope_json(&wrapped, &AstEnvelopeReaderOptions::default()).unwrap();
    assert_eq!(to_json(&read), to_json(&from_json(&to_json(&doc)).unwrap()));
}

#[test]
fn a_higher_major_is_refused_by_version_not_by_the_schema() {
    let payload = envelope(&tree(), &[("astVersion", "\"2.0\"")]);
    let error = from_ast_envelope_json(&payload, &AstEnvelopeReaderOptions::default()).unwrap_err();
    match &error {
        AstEnvelopeError::Version { found, implemented } => {
            assert_eq!(found, "2.0");
            assert_eq!(implemented, AST_CONTRACT_VERSION);
        }
        other => panic!("expected a version error, got {other:?}"),
    }
    assert!(error.to_string().contains("2.0"));
    assert!(error.to_string().contains("1.0"));
}

#[test]
fn a_higher_minor_is_accepted() {
    let payload = envelope(&tree(), &[("astVersion", "\"1.7\"")]);
    from_ast_envelope_json(&payload, &AstEnvelopeReaderOptions::default()).unwrap();
}

#[test]
fn a_higher_minor_is_still_refused_when_it_requires_an_unknown_extension() {
    let payload = envelope(
        &tree(),
        &[
            ("astVersion", "\"1.7\""),
            ("extensions", &format!("[{{\"id\":\"{CITATIONS}\"}}]")),
        ],
    );
    let error = from_ast_envelope_json(&payload, &AstEnvelopeReaderOptions::default()).unwrap_err();
    assert_eq!(error, AstEnvelopeError::Extension(CITATIONS.to_string()));
    assert!(error.to_string().contains(CITATIONS));
}

#[test]
fn an_extension_the_reader_implements_passes() {
    let payload = envelope(
        &tree(),
        &[
            ("astVersion", "\"1.0\""),
            (
                "extensions",
                &format!("[{{\"id\":\"{CITATIONS}\",\"version\":\"1\"}}]"),
            ),
        ],
    );
    from_ast_envelope_json(
        &payload,
        &AstEnvelopeReaderOptions {
            extensions: vec![CITATIONS.to_string()],
            ..AstEnvelopeReaderOptions::default()
        },
    )
    .unwrap();
}

#[test]
fn an_extension_marked_not_required_is_ignored() {
    let payload = envelope(
        &tree(),
        &[
            ("astVersion", "\"1.0\""),
            (
                "extensions",
                &format!("[{{\"id\":\"{CITATIONS}\",\"required\":false}}]"),
            ),
        ],
    );
    from_ast_envelope_json(&payload, &AstEnvelopeReaderOptions::default()).unwrap();
}

#[test]
fn an_absent_vocabulary_means_the_core_one() {
    let bare = envelope(&tree(), &[("astVersion", "\"1.0\"")]);
    let spelled = envelope(
        &tree(),
        &[
            ("astVersion", "\"1.0\""),
            ("vocabulary", &format!("\"{CORE_AST_VOCABULARY}\"")),
        ],
    );
    let reader = AstEnvelopeReaderOptions::default();
    assert_eq!(
        to_json(&from_ast_envelope_json(&bare, &reader).unwrap()),
        to_json(&from_ast_envelope_json(&spelled, &reader).unwrap())
    );
}

#[test]
fn a_foreign_vocabulary_is_refused_rather_than_walked() {
    let foreign = "https://example.invalid/ast/other";
    let payload = envelope(
        &tree(),
        &[
            ("astVersion", "\"1.0\""),
            ("vocabulary", &format!("\"{foreign}\"")),
        ],
    );
    let error = from_ast_envelope_json(&payload, &AstEnvelopeReaderOptions::default()).unwrap_err();
    assert_eq!(error, AstEnvelopeError::Vocabulary(foreign.to_string()));

    from_ast_envelope_json(
        &payload,
        &AstEnvelopeReaderOptions {
            vocabularies: vec![foreign.to_string()],
            ..AstEnvelopeReaderOptions::default()
        },
    )
    .unwrap();
}

#[test]
fn a_malformed_envelope_is_a_shape_failure() {
    let doc = tree();
    let cases: Vec<(String, &str)> = vec![
        (envelope(&doc, &[]), "astVersion"),
        (envelope(&doc, &[("astVersion", "1")]), "astVersion"),
        (envelope(&doc, &[("astVersion", "\"1\"")]), "major.minor"),
        (envelope(&doc, &[("astVersion", "\"01.0\"")]), "major.minor"),
        (envelope(&doc, &[("astVersion", "\"1.00\"")]), "major.minor"),
        (
            envelope(&doc, &[("astVersion", "\"1.0\""), ("astFlavor", "\"x\"")]),
            "astFlavor",
        ),
        (
            envelope(&doc, &[("astVersion", "\"1.0\""), ("vocabulary", "7")]),
            "vocabulary",
        ),
        (
            envelope(&doc, &[("astVersion", "\"1.0\""), ("extensions", "{}")]),
            "extensions",
        ),
        (
            envelope(
                &doc,
                &[
                    ("astVersion", "\"1.0\""),
                    ("extensions", "[{\"name\":\"x\"}]"),
                ],
            ),
            "name",
        ),
        (
            envelope(&doc, &[("astVersion", "\"1.0\""), ("extensions", "[{}]")]),
            "id",
        ),
        ("[]".to_string(), "not an object"),
    ];
    for (payload, needle) in cases {
        let error =
            from_ast_envelope_json(&payload, &AstEnvelopeReaderOptions::default()).unwrap_err();
        assert!(
            matches!(error, AstEnvelopeError::Shape(_)),
            "expected a shape error for {payload}, got {error:?}"
        );
        assert!(
            error.to_string().contains(needle),
            "expected {needle:?} in {error}"
        );
    }
}

#[test]
fn a_missing_document_is_a_shape_failure() {
    let error = from_ast_envelope_json(
        "{\"astVersion\":\"1.0\"}",
        &AstEnvelopeReaderOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(error, AstEnvelopeError::Shape(_)));
    assert!(error.to_string().contains("document"));
}

#[test]
fn an_envelope_failure_is_not_a_decode_failure() {
    let payload = envelope(&tree(), &[("astVersion", "\"2.0\"")]);
    let error = from_ast_envelope_json(&payload, &AstEnvelopeReaderOptions::default()).unwrap_err();
    // The four envelope failures are each their own variant, and none of them
    // is the codec's error - which is what lets a caller tell "I predate this
    // contract" from "this tree is corrupt".
    assert!(!matches!(error, AstEnvelopeError::Document(_)));
    assert!(matches!(error, AstEnvelopeError::Version { .. }));
}

#[test]
fn a_broken_tree_inside_a_good_envelope_is_still_a_decode_failure() {
    let payload = envelope(
        "{\"type\":\"document\",\"children\":[],\"srcByteLength\":0,\"nope\":1}",
        &[("astVersion", "\"1.0\"")],
    );
    let error = from_ast_envelope_json(&payload, &AstEnvelopeReaderOptions::default()).unwrap_err();
    assert!(
        matches!(error, AstEnvelopeError::Document(_)),
        "got {error:?}"
    );
}

/// carve-rs#1965. The schema's pattern matches a major of any width, so a
/// version too large for a `u64` is a well-formed `astVersion` naming a
/// contract this build predates - not a malformed envelope. Measured against
/// carve-js and carve-php, which both answer `Version` here.
#[test]
fn a_major_too_large_for_u64_is_a_version_refusal() {
    let payload = format!(
        r#"{{"astVersion":"99999999999999999999.0","document":{}}}"#,
        r#"{"type":"document","children":[],"srcByteLength":0}"#
    );
    match from_ast_envelope_json(&payload, &AstEnvelopeReaderOptions::default()) {
        Err(AstEnvelopeError::Version { found, implemented }) => {
            assert_eq!(found, "99999999999999999999.0");
            assert_eq!(implemented, AST_CONTRACT_VERSION);
        }
        other => panic!("expected a version refusal, got {other:?}"),
    }
}

/// The comparison is numeric, not lexicographic: `10.0` is above `1.0` even
/// though it sorts below it as text.
#[test]
fn a_wider_major_compares_as_a_number() {
    let payload = format!(
        r#"{{"astVersion":"10.0","document":{}}}"#,
        r#"{"type":"document","children":[],"srcByteLength":0}"#
    );
    assert!(matches!(
        from_ast_envelope_json(&payload, &AstEnvelopeReaderOptions::default()),
        Err(AstEnvelopeError::Version { .. })
    ));
}
