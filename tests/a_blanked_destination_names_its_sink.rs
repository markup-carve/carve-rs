//! The `destination-denied` message names the sink, verbatim.
//!
//! markup-carve/carve#2686 made the message NORMATIVE and spelled one string
//! per sink. Two fields forced that shape: `target` is already its own field,
//! so the message must not repeat it, and `nodeType` is `inline` for both
//! sinks, so the message is the only place the sink kind survives. This engine
//! emitted one string for all three sinks and appended the target to it, so
//! both halves were wrong.
//!
//! The strings are READ FROM the pinned schema rather than written out here:
//! four hand-copied literals across four engines are what drifted in the first
//! place. The schema can only gate MEMBERSHIP in the two-string set, though -
//! which sink takes which is not expressible there - so the pairing is
//! asserted against a real render on every target that emits a destination.

use carve::{CheckedRenderOptions, RenderResult, RenderTarget};
use std::path::{Path, PathBuf};

/// One denied link and one denied image, in that order.
const BOTH: &str = "[report me](javascript:one) and ![report me too](vbscript:two)\n";
const AUTOLINK: &str = "<javascript:alert(1)>\n";

fn schema_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/spec/resources/render-loss-report.schema.json")
}

/// The two messages the schema admits for `destination-denied`, in the order
/// the schema lists them. A missing schema or a missing enum FAILS: a check
/// that skips itself when it cannot find its oracle is no check.
fn admitted_messages() -> Vec<String> {
    let path = schema_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error}\nthe spec submodule is missing - run `git submodule update --init --recursive`",
            path.display()
        )
    });
    let schema: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is not JSON: {error}", path.display()));
    let branch = find_branch(&schema).unwrap_or_else(|| {
        panic!(
            "{} has no destination-denied branch naming its messages - carve#2686 is not in the pin",
            path.display()
        )
    });
    let members = branch
        .pointer("/properties/message/enum")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| {
            panic!(
                "the destination-denied branch of {} carries no message enum",
                path.display()
            )
        });
    members
        .iter()
        .map(|v| {
            v.as_str()
                .unwrap_or_else(|| panic!("a non-string message member {v}"))
                .to_string()
        })
        .collect()
}

/// The subschema whose `code` is pinned to `destination-denied`, wherever the
/// schema puts it, so a reshuffled `$defs` does not silently skip this test.
fn find_branch(node: &serde_json::Value) -> Option<&serde_json::Value> {
    if node
        .pointer("/properties/code/const")
        .and_then(|v| v.as_str())
        == Some("destination-denied")
    {
        return Some(node);
    }
    match node {
        serde_json::Value::Object(map) => map.values().find_map(find_branch),
        serde_json::Value::Array(items) => items.iter().find_map(find_branch),
        _ => None,
    }
}

/// The message for a link or autolink destination, and the one for an image
/// source, picked out of the schema's own members.
fn sink_messages() -> (String, String) {
    let members = admitted_messages();
    assert_eq!(
        members.len(),
        2,
        "the schema admits {} messages, not the two sinks: {members:?}",
        members.len()
    );
    let (image, destination): (Vec<_>, Vec<_>) =
        members.iter().cloned().partition(|m| m.contains("image"));
    assert_eq!(
        (destination.len(), image.len()),
        (1, 1),
        "the two members do not split into one destination and one image message: {members:?}"
    );
    (destination[0].clone(), image[0].clone())
}

fn report(source: &str, target: RenderTarget) -> RenderResult<String> {
    let options = CheckedRenderOptions::default();
    match target {
        RenderTarget::Html => carve::to_html_with_report(source, options),
        RenderTarget::Markdown => carve::to_markdown_with_report(source, options),
        RenderTarget::Plain => carve::to_plain_text_with_report(source, options),
        RenderTarget::Ansi => carve::to_ansi_with_report(source, options),
        RenderTarget::Carve => carve::to_carve_with_report(source, options),
    }
    .expect("a non-strict collection cannot fail")
}

fn messages(source: &str, target: RenderTarget) -> Vec<String> {
    report(source, target)
        .losses
        .iter()
        .map(|loss| loss.message.clone())
        .collect()
}

#[test]
fn each_sink_takes_its_own_canonical_message() {
    let (destination, image) = sink_messages();
    // Both targets that emit both destinations owe one row each, paired by
    // sink and in document order.
    for target in [RenderTarget::Html, RenderTarget::Markdown] {
        assert_eq!(
            messages(BOTH, target),
            vec![destination.clone(), image.clone()],
            "on {}",
            target.as_str()
        );
    }
    // An autolink is a destination, not a sink of its own.
    assert_eq!(
        messages(AUTOLINK, RenderTarget::Html),
        vec![destination.clone()]
    );
}

#[test]
fn the_ansi_target_owes_one_row_and_it_is_the_destination() {
    let (destination, _image) = sink_messages();
    // ANSI prints a link's destination in a parenthetical and renders an image
    // as `[img: alt]`, so the same document owes one row there, not two. This
    // is the likeliest regression when the sink argument is threaded through.
    assert_eq!(messages(BOTH, RenderTarget::Ansi), vec![destination]);
    for target in [RenderTarget::Plain, RenderTarget::Carve] {
        assert!(
            messages(BOTH, target).is_empty(),
            "{} emits no destination",
            target.as_str()
        );
    }
}

#[test]
fn nothing_is_appended_to_the_message() {
    let members = admitted_messages();
    for target in [
        RenderTarget::Html,
        RenderTarget::Markdown,
        RenderTarget::Ansi,
    ] {
        for message in messages(BOTH, target) {
            assert!(
                members.contains(&message),
                "{message:?} on {} is not one of {members:?}",
                target.as_str()
            );
            assert!(
                !message.contains("while rendering"),
                "{message:?} appends the target the row already carries"
            );
        }
    }
}
