//! PART 12 §22: an ingested value the schema calls absent is normalized away.
//!
//! `resources/ast-schema.json` describes a list's `start` as "First number of an
//! ordered list, when it is not 1". That sentence pins the PRODUCER, and this
//! engine already complies - `1. a` parses to a list with no `start` at all. It
//! says nothing on its own about a CONSUMER handed the field anyway, which an
//! editor, a patch tool or a hand-built payload can do, and the three engines
//! took three different positions there before markup-carve/carve#1615 ruled it.
//!
//! §6's round trip is no argument for preserving it: it is scoped to `parse(x)`,
//! a parsed tree, which never carries the value. What decides it is that
//! normalizing is lossless - `start: 1` and no `start` describe the same
//! document and render the same HTML, `1` being the HTML default.
//!
//! WHY THIS IS A HAND-BUILT PAYLOAD AND NOT A CORPUS DOCUMENT. The value is
//! unreachable from Carve source, so no parse-driven corpus document can reach
//! the shape however it is written, which is exactly why the three engines
//! drifted apart here unnoticed.

/// A one-item ordered list, `start` spelled as whatever is asked for. No `pos`
/// anywhere: this is the payload an editor hands back, not one a parser made.
fn payload(start: usize) -> String {
    format!(
        r#"{{"type":"document","srcByteLength":0,"children":[{{"type":"list","ordered":true,"tight":true,"delim":".","start":{start},"items":[{{"type":"list_item","children":[{{"type":"paragraph","children":[{{"type":"text","value":"a"}}]}}]}}]}}]}}"#
    )
}

fn round_tripped(start: usize) -> String {
    let doc = carve::from_json(&payload(start)).expect("ingest");
    carve::to_json(&doc)
}

#[test]
fn a_parsed_ordered_list_never_carries_a_start_of_one() {
    // The half this engine already gets right, asserted so a "fix" that reached
    // the parser instead of the encoder cannot pass here unnoticed.
    let json = carve::to_json(&carve::parse("1. a\n"));

    assert!(
        !json.contains("\"start\""),
        "the parser invented a `start` the schema calls absent: {json}"
    );
}

#[test]
fn an_ingested_start_of_one_is_not_re_emitted() {
    let json = round_tripped(1);

    assert!(
        !json.contains("\"start\""),
        "the encoder re-emitted a `start` the schema says is written only when it is not 1: {json}"
    );
}

#[test]
fn a_start_that_is_not_one_survives_unchanged() {
    // The control that separates §22 from "drop `start` always". A fix that
    // deletes the field outright breaks here rather than shipping. 0 is in the
    // set on purpose: it is falsy in the engines that read this wire format.
    for start in [0usize, 2, 7] {
        let json = round_tripped(start);

        assert!(
            json.contains(&format!("\"start\":{start}")),
            "an ingested start of {start} must survive: {json}"
        );
    }
}

#[test]
fn neither_reading_changes_the_rendered_html() {
    // §22's deciding asymmetry: the two describe the same document. If they ever
    // rendered differently, normalizing would be a loss rather than a cleanup.
    let ingested = carve::from_json(&payload(1)).expect("ingest");
    let with_start = carve::render_html(&ingested).expect("render");
    let without = carve::render_html(&carve::parse("1. a\n")).expect("render");

    assert_eq!(with_start, without);
    assert!(
        !with_start.contains("start="),
        "the renderer spelled the HTML default: {with_start}"
    );
}

#[test]
fn the_normalized_tree_is_what_a_second_ingest_reads() {
    // The point of normalizing at the encoder: what this engine PUBLISHES is a
    // conformant tree, so the next consumer never sees the field either.
    let once = round_tripped(1);
    let twice = carve::to_json(&carve::from_json(&once).expect("re-ingest"));

    assert_eq!(once, twice);
    assert!(!twice.contains("\"start\""));
}
