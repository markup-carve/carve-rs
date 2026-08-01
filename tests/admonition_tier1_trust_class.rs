//! carve issue 431: a fence opened with a non-Tier-1 word (e.g. `::: sidebar`)
//! is a generic container for PROFILE purposes, not a callout. The renderer
//! already drew this line (`render::render_admonition`'s `canonical` check
//! decides `<aside>` vs `<div>`); the profile classifier
//! (`profile::canonical_block_type`) did not, so `denyBlock(["admonition"])`
//! silently stripped every named fence, not just the eight Tier-1 kinds
//! (`note`, `tip`, `warning`, `danger`, `info`, `success`, `example`,
//! `quote`). This mirrors carve-php (markup-carve/carve-php#513) and
//! carve-js, which already draw this line.
//!
//! This is a TRUST-CLASS change only: the published AST for `::: sidebar`
//! must stay `{"type":"admonition","kind":"sidebar"}` (see
//! `sidebar_still_serializes_as_admonition_with_kind` below) and rendering
//! must be byte-identical (see the `rendering_is_unchanged_for` tests) - only
//! what a profile's `admonition` / `div` deny list matches has changed.

use carve::profile_filter::apply_profile;
use carve::{to_html, to_json, Profile};

const NOTE: &str = "::: note\ncallout\n:::\n";
const SIDEBAR: &str = "::: sidebar\ngeneric\n:::\n";
const BARE: &str = ":::\ngeneric\n:::\n";

fn violations_for(src: &str, profile: Profile) -> Vec<String> {
    let doc = carve::parse(src);
    apply_profile(doc, &profile, None)
        .expect("to_text action must not raise")
        .violations
        .into_iter()
        .map(|v| v.node_type)
        .collect()
}

// ---- the four measured cases from carve issue 431 ----

#[test]
fn deny_admonition_still_strips_a_tier1_note() {
    let violations = violations_for(NOTE, Profile::default().deny_block(&["admonition"]));
    assert_eq!(violations, vec!["admonition"], "{violations:?}");
}

#[test]
fn deny_admonition_no_longer_strips_a_non_tier1_sidebar() {
    let violations = violations_for(SIDEBAR, Profile::default().deny_block(&["admonition"]));
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn deny_div_strips_a_non_tier1_sidebar_as_div() {
    let violations = violations_for(SIDEBAR, Profile::default().deny_block(&["div"]));
    assert_eq!(violations, vec!["div"], "{violations:?}");
}

#[test]
fn deny_div_still_strips_a_tier1_note_via_the_subtype_rule() {
    // The supertype rule (`with_supertype("admonition") == ["admonition",
    // "div"]`): a host that wants the old blanket behavior back denies BOTH
    // `admonition` and `div`, but `div` alone already catches every
    // admonition, Tier-1 or not - the reported node type is still
    // `admonition`, not `div`, because that is the node's own canonical name.
    let violations = violations_for(NOTE, Profile::default().deny_block(&["div"]));
    assert_eq!(violations, vec!["admonition"], "{violations:?}");
}

// ---- AST / trust-class separation ----

#[test]
fn sidebar_still_serializes_as_admonition_with_kind() {
    // The profile reclassification above must not leak into the published
    // AST: `::: sidebar` is still an `admonition` node with `kind: "sidebar"`
    // (matching carve-js and resources/ast-schema.json), never a `div`
    // carrying a `kind` field the schema does not allow for `div` (the bug
    // markup-carve/carve-php#543 fixed after carve-php's encoder derived its
    // wire type from the profile classifier).
    let doc = carve::parse(SIDEBAR);
    let json = to_json(&doc);
    assert!(
        json.contains("\"type\":\"admonition\"") || json.contains("\"type\": \"admonition\""),
        "{json}"
    );
    assert!(
        json.contains("\"kind\":\"sidebar\"") || json.contains("\"kind\": \"sidebar\""),
        "{json}"
    );
    assert!(!json.contains("\"type\":\"div\""), "{json}");
    assert!(!json.contains("\"type\": \"div\""), "{json}");
}

// ---- rendering is unchanged (this is a trust-class change, not a rendering
// change - the renderer already drew the Tier-1 line before this fix) ----

#[test]
fn rendering_is_unchanged_for_a_tier1_note() {
    let html = to_html(NOTE);
    assert!(html.contains("<aside"), "{html}");
    assert!(html.contains("admonition note"), "{html}");
}

#[test]
fn rendering_is_unchanged_for_a_non_tier1_sidebar() {
    let html = to_html(SIDEBAR);
    assert!(!html.contains("<aside"), "{html}");
    assert!(html.contains("<div class=\"sidebar\">"), "{html}");
}

#[test]
fn rendering_is_unchanged_for_a_bare_fence() {
    let html = to_html(BARE);
    assert!(!html.contains("<aside"), "{html}");
    assert!(html.contains("<div"), "{html}");
}
