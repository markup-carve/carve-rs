//! PART 9 §17 L7: one consumed boolean spells the looseness no blank line can.
//!
//! A container's preceding block-attribute line may carry the boolean `loose`,
//! which says the container's children render as BLOCKS rather than as inline
//! runs, and which is CONSUMED - it never reaches the output as an HTML
//! attribute. The precedent is PART 12 §15's `header-rows`: a structural fact
//! riding the same line, carried as a boolean, consumed rather than emitted.
//!
//! WHAT IS UNSPELLABLE WITHOUT IT. Looseness is spelled with a blank line, and a
//! blank line needs two things to stand between, so a ONE-ITEM list has no
//! spelling for it. `<li><p>x</p></li>` is what ordinary HTML exports emit
//! (markup-carve/carve#1607), so the shape arrives on routine input.
//!
//! THE DEFINITION-LIST HALF IS NOT HERE. §17 L7 applies to a `<dl>` too, and
//! PART 12 §8 gives `definition_list` a `loose` field for it - but that field
//! landed in the spec at `cfb8d7bf` and this engine's `tests/spec` pin is
//! `3fdfd6e`, so `src/wire_fields.rs`, which is GENERATED from the pinned
//! schema and gated on matching it, cannot name the field yet. An engine
//! publishing it would emit a tree its own ingest refuses under PART 12 §11.
//! `a_definition_list_does_not_take_the_key_yet` below is that state, asserted
//! rather than left silent (markup-carve/carve-rs#1301).

use carve::to_html;

#[test]
fn the_key_loosens_a_one_item_list_and_does_not_reach_the_html() {
    let html = to_html("{loose}\n- Note text.\n");

    // BOTH HALVES, because a fixture that only asserts the looseness still
    // passes while the key leaks through as an attribute.
    assert_eq!(html, "<ul>\n  <li><p>Note text.</p></li>\n</ul>");
    assert!(
        !html.contains("loose"),
        "the key reached the output: {html}"
    );
}

#[test]
fn a_boolean_and_an_empty_value_are_one_key() {
    // PART 4 makes `{loose}` and `{loose=""}` the same attribute, so both are
    // consumed.
    assert_eq!(to_html("{loose=\"\"}\n- x\n"), to_html("{loose}\n- x\n"));
    assert!(!to_html("{loose=\"\"}\n- x\n").contains("loose"));
}

#[test]
fn a_valued_loose_is_not_this_key() {
    // `loose=x` names a value the key does not take, so it stays an ordinary
    // attribute and renders. There is no error state and no half-application.
    let html = to_html("{loose=x}\n- x\n");

    assert!(html.contains("<ul loose=\"x\">"), "{html}");
    assert!(
        html.contains("<li>x</li>"),
        "the list was loosened anyway: {html}"
    );
}

#[test]
fn only_the_key_is_consumed() {
    let html = to_html("{loose .note #n}\n- x\n");

    assert!(
        html.starts_with("<ul id=\"n\" class=\"note\">")
            || html.starts_with("<ul class=\"note\" id=\"n\">"),
        "{html}"
    );
    assert!(!html.contains("loose"), "{html}");
    assert!(html.contains("<li><p>x</p></li>"), "{html}");
}

#[test]
fn an_attribute_line_carrying_only_the_key_leaves_no_attributes_behind() {
    // The emptied set must not be attached: `attrs: {}` on a node the author
    // gave no attributes is a tree the same document written without the key
    // does not produce, and `fmt` then stopped round-tripping to its own parse.
    let json = carve::to_json(&carve::parse("{loose}\n- x\n"));

    assert!(!json.contains("\"attrs\""), "{json}");
    assert!(json.contains("\"tight\":false"), "{json}");
}

#[test]
fn the_axis_lands_in_the_field_that_already_states_it() {
    // §17 L7: a loosened LIST sets `list.tight` false, and nothing else is
    // published - the axis is total there, so no second field is needed.
    assert!(carve::to_json(&carve::parse("{loose}\n- x\n")).contains("\"tight\":false"));
    assert!(carve::to_json(&carve::parse("- x\n")).contains("\"tight\":true"));
}

#[test]
fn ordered_and_nested_containers_take_the_same_key_at_the_same_placement() {
    assert_eq!(
        to_html("{loose}\n1. Note.\n"),
        "<ol>\n  <li><p>Note.</p></li>\n</ol>"
    );

    // An attribute line at a sub-list's indentation loosens the SUB-LIST and
    // not its parent.
    let html = to_html("- a\n\n  {loose}\n  - sub\n");

    assert!(
        html.contains("<li><p>sub</p></li>"),
        "the sub-list stayed tight: {html}"
    );
    assert!(!html.contains("loose"), "{html}");
    assert!(
        !html.starts_with("<ul>\n  <li><p>a"),
        "the parent was loosened too: {html}"
    );
}

#[test]
fn on_a_container_with_no_such_axis_the_name_is_not_reserved() {
    // The clause adds a meaning at the positions that have one and reserves the
    // name nowhere else.
    assert!(to_html("{loose}\n> q\n").contains("<blockquote loose=\"\">"));
    assert!(to_html("{loose}\nx\n").contains("<p loose=\"\">"));
}

#[test]
fn redundant_use_is_a_legal_no_op() {
    // `loose` on a list the blank lines already loosened changes nothing.
    // Rejecting it would make the key context-sensitive, and a producer that
    // always emits it is simpler than one that has to decide.
    assert_eq!(to_html("{loose}\n- a\n\n- b\n"), to_html("- a\n\n- b\n"));
}

/// THE WRITER HALF OF §17 L7 IS NOT IMPLEMENTED HERE, and this is that state
/// asserted rather than left silent.
///
/// L7 says the canonical writer emits the key only where the blank-line
/// spelling cannot say it - a ONE-ITEM loose list. Implementing it changes the
/// output of a SHARED cross-engine fixture that currently pins the loss:
/// `tests/spec/tests/html-import/derived-endnotes-section` imports
/// `<ol><li><p>Note text.</p></li></ol>` and expects `---\n\n1. Note text.\n`,
/// which reads back tight. That expectation is the shape L7 exists for
/// (markup-carve/carve#1607), so moving it is a spec-repo change to be made
/// once for all three engines rather than unilaterally here, and the sibling
/// engine work (carve-js#1401, carve-php#1634) is scoped to the parser and the
/// renderer for the same reason.
///
/// Measured while writing it: a re-parse test decides the rule correctly - it
/// declines corpus `05-lists-11`, a one-item ordered list whose ITEM holds two
/// paragraphs and is therefore already loose on the page - and the only
/// documents it does decorate are the ones that were losing the looseness.
#[test]
fn the_writer_does_not_spell_the_looseness_yet() {
    let one_item = carve::from_json(
        r#"{"type":"document","srcByteLength":0,"children":[{"type":"list","ordered":false,"tight":false,"bulletChar":"-","items":[{"type":"list_item","children":[{"type":"paragraph","children":[{"type":"text","value":"x"}]}]}]}]}"#,
    )
    .expect("ingest");
    let written = carve::render_carve(&one_item).expect("write");

    assert_eq!(written, "- x\n");
    assert_ne!(
        to_html(&written),
        carve::render_html(&one_item).expect("render"),
        "the writer half landed - delete this test and pin the round trip instead"
    );

    // The control that will still hold when the writer half lands: a two-item
    // loose list already says it with the blank line between the items, so the
    // key would be an idle mark there under PART 11 §2.
    assert!(!carve::render_carve(&carve::parse("- a\n\n- b\n"))
        .expect("write")
        .contains("loose"));
}

#[test]
fn the_key_loosens_a_definition_list_and_does_not_reach_the_html() {
    // §17 L7 on the OTHER container that has the axis. The wrapper is what
    // moves; the key is consumed and never becomes an attribute.
    let html = to_html("{loose}\n:: Term\n:  Definition.\n");

    assert_eq!(
        html,
        "<dl>\n  <dt>Term</dt>\n  <dd><p>Definition.</p></dd>\n</dl>"
    );
}

#[test]
fn the_definition_list_key_does_not_reach_the_html() {
    assert!(!to_html("{loose}\n:: Term\n:  Definition.\n").contains("loose"));
}

#[test]
fn a_definition_lists_looseness_is_published_only_when_spelled() {
    // PART 12 §8 types the field `const: true`, so it is written only when the
    // key said so. A `<dl>` that derives its own wrappers publishes nothing,
    // because that fact is re-derivable from the description's block count.
    assert!(carve::to_json(&carve::parse("{loose}\n:: T\n:  d\n")).contains("\"loose\":true"));
    assert!(!carve::to_json(&carve::parse(":: T\n:  d\n")).contains("loose"));
    assert!(!carve::to_json(&carve::parse(":: T\n:  d\n\n   second\n")).contains("loose"));
}

#[test]
fn the_definition_lists_looseness_survives_an_ast_round_trip() {
    // The half the LIST arm gets for free from `tight`: without a field of its
    // own the wrapper was underivable, so an ingested tree rendered a shape the
    // document it came from did not have.
    let doc = carve::parse("{loose}\n:: T\n:  d\n");
    let back = carve::from_json(&carve::to_json(&doc)).expect("ingest");

    assert_eq!(
        carve::render_html(&back).expect("render"),
        carve::render_html(&doc).expect("render")
    );
}

#[test]
fn a_definition_list_key_line_carrying_only_the_key_leaves_no_attributes_behind() {
    assert!(!carve::to_json(&carve::parse("{loose}\n:: T\n:  d\n")).contains("\"attrs\""));
}

#[test]
fn only_the_definition_list_key_is_consumed() {
    let html = to_html("{loose .note}\n:: T\n:  d\n");

    assert!(html.starts_with("<dl class=\"note\">"), "{html}");
    assert!(!html.contains("loose"), "{html}");
    assert!(html.contains("<dd><p>d</p></dd>"), "{html}");
}

#[test]
fn a_valued_loose_is_not_the_definition_list_key_either() {
    let html = to_html("{loose=x}\n:: T\n:  d\n");

    assert!(html.contains("<dl loose=\"x\">"), "{html}");
    assert!(html.contains("<dd>d</dd>"), "the list was loosened anyway: {html}");
}

#[test]
fn a_redundant_definition_list_key_is_a_legal_no_op_in_the_render() {
    // A description that already holds two blocks takes the wrapper either way,
    // so the key changes nothing an HTML reader can see. It still changes the
    // TREE, which is what the writer arm below is about.
    assert_eq!(
        to_html("{loose}\n:: T\n:  a\n\n   b\n"),
        to_html(":: T\n:  a\n\n   b\n")
    );
}
