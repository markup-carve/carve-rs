//! PART 12 section 11: a property the schema does not name is REFUSED on
//! ingest (carve-rs#691).
//!
//! This engine's codec names every field at both ends, so an unknown property
//! was simply not carried - which made its OUTPUT conformant, unlike an engine
//! that echoes the property back, but left the INPUT side silent: the payload
//! was accepted and the caller was told nothing.
//!
//! The clause rules that out for the reason section 9(b) gives about depth:
//! "an ingest that accepts a tree and then silently renders only part of it is
//! the worst of the three, because the caller is told nothing".

use carve::{from_json, to_json, Options};

const SOURCE: &str = "# Heading\n\ntext with *emphasis* and a [link](/u)\n\n- item\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\nsee[^a]\n\n[^a]: note\n";

fn published(source: &str) -> String {
    to_json(&carve::parse_with_options(
        source,
        &Options::default().with_positions(true),
    ))
}

/// Inject `bogusXyz` on the nth object that carries a `"type"`, textually.
///
/// The engine has no public JSON value type, so this edits the serialized form
/// - which is also the shape a caller actually hands over.
fn inject(json: &str, nth: usize) -> Option<String> {
    let needle = "\"type\":";
    let mut seen = 0;
    let mut at = 0;
    while let Some(found) = json[at..].find(needle) {
        let index = at + found;
        if seen == nth {
            return Some(format!(
                "{}\"bogusXyz\":\"leak\",{}",
                &json[..index],
                &json[index..]
            ));
        }
        seen += 1;
        at = index + needle.len();
    }
    None
}

#[test]
fn every_node_kind_the_document_holds_is_refused() {
    let json = published(SOURCE);
    let mut refused = 0;
    let mut nth = 0;
    while let Some(payload) = inject(&json, nth) {
        let error =
            from_json(&payload).expect_err(&format!("node {nth} accepted a stray property"));
        assert!(
            error.to_string().contains("bogusXyz"),
            "node {nth}: {error}"
        );
        refused += 1;
        nth += 1;
    }
    // The control on the FIXTURE: a document that grew only one node kind would
    // make the sweep look thorough while proving one case.
    assert!(refused > 15, "only {refused} nodes carried a type");
}

#[test]
fn the_error_names_the_property_and_where_it_sat() {
    let json = published("# Heading\n");
    let payload = inject(&json, 1).expect("the heading node carries a type");

    let error = from_json(&payload).expect_err("a stray property was accepted");
    let message = error.to_string();
    assert!(message.contains("bogusXyz"), "{message}");
    assert!(message.contains("heading"), "{message}");
}

#[test]
fn a_stray_key_on_an_object_that_hangs_off_a_node_is_refused() {
    let json = published("::: note\ncontent\n:::\n");
    let payload = json.replace("\"pos\":{", "\"pos\":{\"bogusXyz\":1,");
    assert_ne!(payload, json, "the fixture published no pos to edit");

    let error = from_json(&payload).expect_err("a stray key on pos was accepted");
    assert!(error.to_string().contains("bogusXyz"), "{error}");
}

#[test]
fn a_tree_this_engine_published_still_round_trips() {
    // The control. Every assertion above passes for a decoder that refuses
    // EVERYTHING, and PART 12 section 6 is what such a decoder would break.
    let json = published(SOURCE);

    let doc = from_json(&json).expect("this engine's own tree is readable");
    assert_eq!(to_json(&doc), json);
}

#[test]
fn the_legacy_footnote_id_is_still_read() {
    // Section 11's one carve-out: a property the ingest UNDERSTANDS, published
    // by this engine's siblings before section 7 settled on `label`, decoded
    // onto the named field. Refusing those stored trees would not protect a
    // caller from a half-read tree.
    let payload = r#"{"type":"document","srcByteLength":0,"children":[{"type":"footnote","id":"a","children":[]}]}"#;

    assert!(from_json(payload).is_ok());
}
