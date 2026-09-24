//! CARVE-P12-055's `block_extension`, and the render-stage carrier that used to
//! share its wire name.
//!
//! Both directions were closed before carve-rs#1865: the engine wrote a
//! `block_extension` carrying `children`, which the schema does not name, and it
//! refused a conforming one for a missing `children`. Each half is pinned here on
//! its own, so reverting one fails its own cases.

use carve::ast::{BlockNode, Document};

/// A conforming payload: qualified name, a version, the required fallback, and an
/// opaque payload whose value carries a `type` of the extension's own.
const CONFORMING: &str = r#"{
  "type": "document",
  "srcByteLength": 0,
  "children": [
    {
      "type": "block_extension",
      "name": "org.example.diagram",
      "version": "2",
      "fallback": {
        "type": "paragraph",
        "children": [{ "type": "text", "value": "A flow chart." }]
      },
      "payload": { "format": "application/json", "value": { "type": "swimlane", "lanes": 3 } }
    }
  ]
}"#;

fn decode(json: &str) -> Document {
    carve::from_json(json).expect("a conforming block extension decodes")
}

fn only_block(doc: &Document) -> &BlockNode {
    assert_eq!(doc.children.len(), 1, "one block");
    &doc.children[0]
}

#[test]
fn a_conforming_block_extension_decodes() {
    let doc = decode(CONFORMING);
    let BlockNode::BlockExtension(node) = only_block(&doc) else {
        panic!("expected a block_extension, got {:?}", only_block(&doc));
    };
    assert_eq!(node.name, "org.example.diagram");
    assert_eq!(node.version.as_deref(), Some("2"));
    assert!(matches!(*node.fallback, BlockNode::Paragraph(_)));
    let payload = node.payload.as_ref().expect("the payload is carried");
    assert_eq!(payload.format, "application/json");
    assert_eq!(
        payload.value.as_deref(),
        Some(r#"{"lanes":3,"type":"swimlane"}"#),
        "held as JSON text, opaque"
    );
}

#[test]
fn an_opaque_payload_is_opaque() {
    // The trap carve-php reported in its #2324: a walk that builds a node out of
    // anything carrying a string `type` refuses a legitimate payload. Here the
    // §11 unknown-field check was the walk - it reaches every nested object - so
    // a payload value whose `type` names a REAL node type with a field that node
    // does not name is the sharpest case: `swimlane` is unknown and was waved
    // through, `text` is known and was not.
    for value in [
        r#"{"type":"swimlane","lanes":3}"#,
        r#"{"type":"text","content":"not a Carve text node"}"#,
        r#"{"type":"paragraph","children":"a string, not an array"}"#,
        r#"[{"type":"code_block","attrs":{"nope":1}}]"#,
        r#""a bare string""#,
        "42",
    ] {
        let json = format!(
            r#"{{"type":"document","srcByteLength":0,"children":[{{"type":"block_extension","name":"org.example.diagram","fallback":{{"type":"paragraph","children":[]}},"payload":{{"format":"application/json","value":{value}}}}}]}}"#
        );
        carve::from_json(&json).unwrap_or_else(|e| {
            panic!("an opaque payload is opaque, but {value} was refused: {e}")
        });
    }
}

#[test]
fn a_payload_without_a_format_is_refused() {
    // A reader cannot tell whether it can parse the value at all without one.
    let json = r#"{"type":"document","srcByteLength":0,"children":[{"type":"block_extension","name":"org.example.diagram","fallback":{"type":"paragraph","children":[]},"payload":{"value":1}}]}"#;
    let error = carve::from_json(json).expect_err("a payload without a format is refused");
    assert!(
        error.to_string().contains("format"),
        "the error names the missing field: {error}"
    );
}

#[test]
fn a_block_extension_without_a_fallback_is_refused() {
    let json = r#"{"type":"document","srcByteLength":0,"children":[{"type":"block_extension","name":"org.example.diagram"}]}"#;
    let error = carve::from_json(json).expect_err("the fallback is required");
    assert!(
        error.to_string().contains("fallback"),
        "the error names the missing field: {error}"
    );
}

#[test]
fn children_is_refused_because_the_schema_does_not_name_it() {
    // The shape this engine used to WRITE. PART 12 §11.
    let json = r#"{"type":"document","srcByteLength":0,"children":[{"type":"block_extension","name":"citations","children":[]}]}"#;
    let error = carve::from_json(json).expect_err("children is not a property of this node");
    assert!(
        error.to_string().contains("children"),
        "the error names the offending property: {error}"
    );
}

#[test]
fn what_the_engine_writes_reads_back() {
    let doc = decode(CONFORMING);
    let written = carve::to_json(&doc);
    let round_tripped = carve::from_json(&written).expect("the engine reads back what it writes");
    assert_eq!(
        carve::to_json(&round_tripped),
        written,
        "the second pass is identical"
    );
    assert!(
        written.contains(r#""fallback":{"type":"paragraph""#),
        "the fallback is published as the schema names it: {written}"
    );
    let value: serde_json::Value = serde_json::from_str(&written).expect("valid JSON");
    let node = value.pointer("/children/0").expect("the one block");
    assert!(
        node.get("children").is_none(),
        "no `children` reaches the wire on this node - that is what the schema refuses: {written}"
    );
}

#[test]
fn every_target_renders_the_fallback() {
    let doc = decode(CONFORMING);
    assert_eq!(
        carve::render_html(&doc).expect("renderable").trim(),
        "<p>A flow chart.</p>",
        "HTML renders the fallback, with no wrapper of its own"
    );
    assert_eq!(
        carve::render_markdown(&doc).expect("renderable").trim(),
        "A flow chart."
    );
    assert_eq!(
        carve::render_plain_text(&doc).expect("renderable").trim(),
        "A flow chart."
    );
    assert_eq!(
        carve::render_ansi(&doc).expect("renderable").trim(),
        "A flow chart."
    );
    assert_eq!(
        carve::render_carve(&doc).expect("spellable").trim(),
        "A flow chart.",
        "the canonical writer has no spelling for the extension, so it writes what the \
         document means to a reader without it"
    );
}

#[test]
fn the_prosemirror_bridge_puts_the_fallback_in_its_place_and_says_so() {
    let doc = decode(CONFORMING);
    let bridged = carve::to_prosemirror(&doc);
    let value: serde_json::Value = serde_json::from_str(&bridged.json).expect("PM JSON");
    assert_eq!(
        value.pointer("/content/0/type").and_then(|v| v.as_str()),
        Some("paragraph"),
        "the fallback takes the node's place rather than the whole node being dropped"
    );
    assert!(
        bridged.degraded.contains_key("block_extension"),
        "the lost identity, version and payload are reported: {:?} / {:?}",
        bridged.degraded,
        bridged.dropped
    );
    assert!(
        !bridged.dropped.contains_key("block_extension"),
        "nothing is dropped: {:?}",
        bridged.dropped
    );
}

#[test]
fn a_profile_names_it_and_denies_it_under_its_own_name() {
    let doc = decode(CONFORMING);
    let BlockNode::BlockExtension(_) = only_block(&doc) else {
        panic!("expected a block_extension");
    };
    assert_eq!(
        carve::profile::canonical_block_type(only_block(&doc)),
        Some("block_extension"),
        "it gates under its own name, not under the carrier's"
    );
}

// --- the other half: the render-stage carrier -------------------------------

/// A document whose prepared tree holds a real `ExtensionCarrier`. The carrier is
/// built by `before_render`, never by `parse`, which is why every existing
/// round-trip test walked past the collision.
fn prepared_with_a_carrier() -> Document {
    let details = carve::Details::new();
    let options = carve::Options::new().with_extension(&details);
    let doc = carve::parse_with_options(
        "::: details \"Why\"\nBecause of the thing.\n:::\n",
        &options,
    );
    let prepared =
        carve::prepare_document_for_render(doc, &options, carve::Mode::Interactive, true)
            .expect("no profile, so no violation");
    assert!(
        matches!(
            prepared.children.first(),
            Some(BlockNode::ExtensionCarrier(_))
        ),
        "the fixture has to hold a carrier, or it pins nothing: {:?}",
        prepared.children
    );
    prepared
}

#[test]
fn the_carrier_the_engine_writes_reads_back() {
    let prepared = prepared_with_a_carrier();
    let written = carve::to_json(&prepared);
    let decoded = carve::from_json(&written).expect("the engine reads back what it writes");
    assert!(
        matches!(decoded.children.first(), Some(BlockNode::Div(_))),
        "the carrier is published as the div it degrades to on every other target: {written}"
    );
    assert_eq!(
        carve::to_json(&decoded),
        written,
        "and the second pass is identical"
    );
}

#[test]
fn the_carrier_does_not_claim_the_schemas_name() {
    let written = carve::to_json(&prepared_with_a_carrier());
    assert!(
        !written.contains("block_extension"),
        "the wire name belongs to CARVE-P12-055's node: {written}"
    );
    assert!(
        written.contains("Because of the thing."),
        "the carrier's content is published, not dropped: {written}"
    );
}
