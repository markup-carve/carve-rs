//! carve#6298 and carve#6299: the 0.1 streaming render and incremental parse
//! calls over the 0.2 engine.

mod common;

use carve::api::{self, Options};
use carve::{
    StreamOutcome, TextChange, parse_snapshot, parse_snapshot_with_identity, reparse,
    try_render_html_streaming,
};
use common::{options_of, parse_json};
use serde_json::Value;

fn clause_cases() -> Vec<Value> {
    parse_json(include_str!("../spec-conformance/clause-cases.json"))["cases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| matches!(c["kind"].as_str(), None | Some("parse" | "render")))
        .filter(|c| c["input"].is_string())
        .filter(|c| c["options"].get("extensions").is_none() && c.get("host").is_none())
        .cloned()
        .collect()
}

fn streamed(source: &str, options: &Options) -> (StreamOutcome, Vec<String>) {
    let mut chunks = Vec::new();
    let outcome = try_render_html_streaming(source, options, |chunk| {
        chunks.push(chunk.to_owned());
    });
    (outcome, chunks)
}

fn fresh(source: &str) -> (api::Document, String) {
    let parsed = api::parse_with_layout(source, &Options::default()).unwrap();
    (parsed.tree.into_document(), parsed.layout.to_string())
}

#[test]
fn streaming_equals_the_render_over_the_clause_cases() {
    let cases = clause_cases();
    assert!(cases.len() > 500, "{}", cases.len());
    for case in &cases {
        let input = case["input"].as_str().unwrap_or_default();
        let options = options_of(&case["options"]);
        let (outcome, chunks) = streamed(input, &options);
        if let Ok(html) = api::render(input, &options) {
            assert_eq!(outcome, StreamOutcome::Complete, "{}", case["id"]);
            assert_eq!(chunks.concat(), html, "{}", case["id"]);
        } else {
            assert_eq!(outcome, StreamOutcome::NeedsAst, "{}", case["id"]);
            assert!(chunks.is_empty(), "{}", case["id"]);
        }
    }
}

#[test]
fn large_output_arrives_in_bounded_utf8_chunks() {
    for source in [
        "word & tëxt ".repeat(20_000).trim_end().to_owned(),
        format!("```\n{}\n```\n", "<é>".repeat(30_000)),
        "- item with *strong*\n".repeat(8_000),
    ] {
        let (outcome, chunks) = streamed(&source, &Options::default());
        assert_eq!(outcome, StreamOutcome::Complete);
        assert!(chunks.len() > 10);
        assert!(chunks.iter().all(|chunk| chunk.len() <= 4096));
        assert_eq!(
            chunks.concat(),
            api::render(source.as_str(), &Options::default()).unwrap()
        );
    }
}

#[test]
fn a_refused_render_never_reaches_the_sink() {
    let options = Options {
        strict_losses: true,
        ..Options::default()
    };
    let source = "`x`{=latex}";
    assert!(api::render(source, &options).is_err());
    let result = try_render_html_streaming(source, &options, |_| {
        panic!("a refused render reached the sink");
    });
    assert_eq!(result, StreamOutcome::NeedsAst);
}

#[test]
fn empty_document_calls_once_and_a_panicking_sink_leaves_no_state() {
    let (outcome, chunks) = streamed("", &Options::default());
    assert_eq!(outcome, StreamOutcome::Complete);
    assert_eq!(chunks, vec![String::new()]);
    let source = "[label][ref]\n\n[ref]: /target\n";
    assert!(
        std::panic::catch_unwind(|| {
            try_render_html_streaming(source, &Options::default(), |_| panic!("sink failed"));
        })
        .is_err()
    );
    let (outcome, chunks) = streamed(source, &Options::default());
    assert_eq!(outcome, StreamOutcome::Complete);
    assert_eq!(
        chunks.concat(),
        api::render(source, &Options::default()).unwrap()
    );
}

fn edit(range: std::ops::Range<usize>, replacement: &str) -> TextChange {
    TextChange {
        range,
        replacement: replacement.into(),
    }
}

/// The step's tree equals a fresh parse; its layout is built by the same
/// call from an equal tree, which [`assert_fresh`] checks on the reuse path.
fn assert_fresh_tree(state: &carve::IncrementalParse, label: &str) {
    let document = api::parse(state.snapshot.source(), &Options::default())
        .unwrap()
        .into_document();
    assert_eq!(state.document, document, "{label}: tree differs");
}

/// Each step's tree and layout equal a fresh parse of the step's source.
fn assert_fresh(state: &carve::IncrementalParse, label: &str) {
    let (document, layout) = fresh(state.snapshot.source());
    assert_eq!(state.document, document, "{label}: tree differs");
    assert_eq!(state.source_layout_json, layout, "{label}: layout differs");
}

#[test]
fn single_character_edits_at_every_offset_match_fresh_parses() {
    let mut cases = clause_cases()
        .iter()
        .filter_map(|c| c["input"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    cases.extend([
        "first\n\nmiddle text\n\nlast".into(),
        "één\n\nMitte\n\n終わり".into(),
    ]);
    let mut checked = 0;
    for input in &cases {
        let mut state = parse_snapshot(input).unwrap();
        let mut offsets = input
            .char_indices()
            .map(|(at, _)| at)
            .chain([input.len()])
            .collect::<Vec<_>>();
        // Every offset of the short cases; 8 spread offsets of the few long ones.
        if offsets.len() > 256 {
            let step = offsets.len() / 8;
            offsets = offsets.into_iter().step_by(step).collect();
        }
        for &at in &offsets {
            for inserted in ["x", "\n"] {
                state = reparse(state.snapshot, &[edit(at..at, inserted)]).unwrap();
                assert_fresh_tree(&state, &format!("{input:?} insert {inserted:?} at {at}"));
                state = reparse(state.snapshot, &[edit(at..at + inserted.len(), "")]).unwrap();
                assert_fresh_tree(&state, &format!("{input:?} undo at {at}"));
                checked += 2;
            }
            if let Some(ch) = input.get(at..).and_then(|rest| rest.chars().next()) {
                let removed = at..at + ch.len_utf8();
                state = reparse(state.snapshot, &[edit(removed.clone(), "")]).unwrap();
                assert_fresh_tree(&state, &format!("{input:?} delete at {at}"));
                state = reparse(state.snapshot, &[edit(at..at, &ch.to_string())]).unwrap();
                assert_fresh_tree(&state, &format!("{input:?} restore at {at}"));
                checked += 2;
            }
        }
        assert_eq!(state.snapshot.source(), input);
    }
    assert!(checked > 30_000, "{checked}");
}

#[test]
fn paragraph_edit_reuses_other_blocks_and_preserves_positions() {
    for (source, replacement) in [
        ("first\n\nmiddle\n\nlast\n", "longer text"),
        ("één\n\nMitte\n\n終わり\n", "日本語"),
    ] {
        let first = parse_snapshot_with_identity(source).unwrap();
        let start = source.find("\n\n").unwrap() + 2;
        let end = start + source[start..].find('\n').unwrap();
        let changed = reparse(first.snapshot, &[edit(start..end, replacement)]).unwrap();
        assert_fresh(&changed, source);
        assert!(changed.reused_previous_tree);
        assert_eq!(changed.parsed_source_bytes, replacement.len());
    }
}

#[test]
fn complex_edits_fall_back_and_noop_reuses_everything() {
    for replacement in ["# Heading", "[ref]: /url", "first\n\nsecond", "1. item", ""] {
        let first = parse_snapshot("before\n\ntext\n\nafter").unwrap();
        let changed = reparse(first.snapshot, &[edit(8..12, replacement)]).unwrap();
        assert_fresh(&changed, replacement);
        assert!(!changed.reused_previous_tree, "{replacement:?}");
        assert!(changed.parsed_source_bytes >= changed.snapshot.source().len());
    }
    for source in ["plain text", "# Heading\n\n[link](/url)"] {
        let first = parse_snapshot(source).unwrap();
        let changed = reparse(first.snapshot, &[]).unwrap();
        let plain = source == "plain text";
        assert_eq!(changed.reused_previous_tree, plain);
        assert_eq!(
            changed.parsed_source_bytes,
            if plain { 0 } else { source.len() }
        );
        assert_eq!(changed.document, first.document);
    }
}

#[test]
fn repeated_local_edits_match_fresh_parses() {
    let mut state = parse_snapshot("first\n\ntext\n\nlast").unwrap();
    for index in 0..100 {
        let end = state.snapshot.source()[7..].find('\n').unwrap() + 7;
        state = reparse(
            state.snapshot,
            &[edit(7..end, &format!("paragraph {index} é"))],
        )
        .unwrap();
        assert_fresh(&state, &index.to_string());
        assert!(state.reused_previous_tree);
    }
}

#[test]
fn a_snapshot_accepts_non_overlapping_utf8_byte_edits() {
    let first = parse_snapshot("# Title\n\nBody.\n").unwrap();
    let second = reparse(first.snapshot, &[edit(9..13, "Text")]).unwrap();
    assert_eq!(second.snapshot.source(), "# Title\n\nText.\n");
    assert_eq!(second.changed_source, vec![9..13]);
    assert_fresh(&second, "title");
}

#[test]
fn overlapping_and_non_boundary_edits_are_rejected() {
    let snapshot = parse_snapshot("éx").unwrap().snapshot;
    assert!(reparse(snapshot.clone(), &[edit(1..2, "")]).is_err());
    assert!(reparse(snapshot.clone(), &[edit(0..1, ""), edit(0..2, "")]).is_err());
    let error = reparse(snapshot, &[edit(1..2, ""), edit(9..10, "")]).unwrap_err();
    assert_eq!(error.to_string(), "text change is out of bounds");
}

fn id_at(identity: &carve::api::NodeIdentity, path: &str) -> String {
    identity
        .nodes
        .iter()
        .find(|node| node.path == path)
        .unwrap()
        .id
        .clone()
}

#[test]
fn batch_edits_preserve_insertion_order_and_untouched_identity() {
    let first = parse_snapshot_with_identity("one\n\ntwo\n\nthree\n\nfour").unwrap();
    let middle = id_at(first.node_identity.as_ref().unwrap(), "/children/1");
    let second = reparse(
        first.snapshot,
        &[
            edit(0..0, "A"),
            edit(0..0, "B"),
            edit(0..3, "ONE"),
            edit(17..21, "4"),
        ],
    )
    .unwrap();
    assert_eq!(second.snapshot.source(), "ABONE\n\ntwo\n\nthree\n\n4");
    assert_eq!(
        id_at(second.node_identity.as_ref().unwrap(), "/children/1"),
        middle
    );
}

#[test]
fn unchanged_node_keeps_id_after_prior_insertion() {
    let first = parse_snapshot_with_identity("alpha\n\nbeta").unwrap();
    let old = first.node_identity.clone().unwrap();
    let second = reparse(first.snapshot, &[edit(0..0, "new\n\n")]).unwrap();
    let next = second.node_identity.unwrap();
    assert_eq!(next.session, old.session);
    assert_eq!(id_at(&next, "/children/2"), id_at(&old, "/children/1"));
    assert_eq!(id_at(&next, ""), id_at(&old, ""));
}

#[test]
fn edited_node_retires_its_id_within_session() {
    let first = parse_snapshot_with_identity("alpha").unwrap();
    let paragraph = id_at(first.node_identity.as_ref().unwrap(), "/children/0");
    let second = reparse(first.snapshot, &[edit(0..5, "omega")]).unwrap();
    assert!(
        second
            .node_identity
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .all(|entry| entry.id != paragraph)
    );
    let third = reparse(second.snapshot, &[edit(0..5, "alpha")]).unwrap();
    assert!(
        third
            .node_identity
            .unwrap()
            .nodes
            .iter()
            .all(|entry| entry.id != paragraph)
    );
}

#[test]
fn identity_paths_address_every_node_of_the_encoded_tree() {
    let first = parse_snapshot_with_identity("# Title\n\n- *a*\n- b\n\n> q").unwrap();
    let identity = first.node_identity.unwrap();
    let tree: Value = serde_json::from_str(&api::encode_tree(
        &api::parse("# Title\n\n- *a*\n- b\n\n> q", &Options::default()).unwrap(),
    ))
    .unwrap();
    for entry in &identity.nodes {
        let node = tree.pointer(&entry.path).unwrap();
        assert!(node.get("type").is_some(), "{}", entry.path);
    }
    let mut ids = identity.nodes.iter().map(|n| &n.id).collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), identity.nodes.len());
}
