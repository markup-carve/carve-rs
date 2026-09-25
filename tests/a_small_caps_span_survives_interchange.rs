//! `small_caps` reaches this engine, renders, writes and round trips
//! (carve-rs#1859, CARVE-P12-050).
//!
//! It is interchange-only: Carve 0.1 source has no spelling for the wrapper, so
//! the parser never synthesizes one and it arrives from a bridge, an importer or
//! an editing API. The clause pins each target, and the interesting one is the
//! canonical Carve writer: it MUST write the children without the wrapper, keep
//! `attrs` on an ordinary attributed span, and MUST NOT discard the children,
//! change their letter case, or invent small-caps source syntax.
//!
//! It shares `EmphasisKind` with the marks that DO have a spelling, because the
//! node shape is the same - children plus attributes - and every walker already
//! reaches it there.

use carve::profile::canonical_inline_type;
use carve::{apply_profile, DisallowedAction, EmphasisKind, InlineNode, Profile};

fn doc(inline: &str) -> String {
    format!(
        r#"{{"type":"document","srcByteLength":0,"children":[
            {{"type":"paragraph","children":[{inline}]}}]}}"#
    )
}

const BARE: &str = r#"{"type":"small_caps","children":[{"type":"text","value":"nasa"}]}"#;

/// The id comes before the class in the author's slot order, which is what the
/// base-class merge has to preserve.
// `r##` because the JSON holds `"#id"`, and `"#` would close an `r#` string.
const WITH_ATTRS: &str = r##"{"type":"small_caps","children":[{"type":"text","value":"nasa"}],"attrs":{"id":"agency","classes":["loud"],"order":["#id",".class"]}}"##;

fn decoded(inline: &str) -> carve::Document {
    carve::from_json(&doc(inline)).expect("the tree decodes")
}

fn first_inline(document: &carve::Document) -> InlineNode {
    let carve::BlockNode::Paragraph(p) = &document.children[0] else {
        panic!("the first block is a paragraph");
    };
    p.children[0].clone()
}

#[test]
fn it_decodes_as_its_own_kind() {
    let InlineNode::Emphasis(node) = first_inline(&decoded(BARE)) else {
        panic!("small_caps decodes into the emphasis family");
    };
    assert_eq!(node.kind, EmphasisKind::SmallCaps);
    assert_eq!(node.children.len(), 1);
    assert_eq!(
        canonical_inline_type(&first_inline(&decoded(BARE))),
        Some("small_caps")
    );
}

#[test]
fn it_round_trips_under_its_own_name() {
    for inline in [BARE, WITH_ATTRS] {
        let document = decoded(inline);
        let republished = carve::to_json(&document);
        assert!(
            republished.contains(r#""type":"small_caps""#),
            "it comes back out as itself: {republished}"
        );
        let again = carve::from_json(&republished).expect("the republished tree decodes");
        assert_eq!(first_inline(&again), first_inline(&document));
    }
}

#[test]
fn html_wraps_it_in_a_smallcaps_span() {
    let html = carve::render_html(&decoded(BARE)).expect("renders");
    assert!(
        html.contains("<span class=\"smallcaps\">nasa</span>"),
        "{html}"
    );
}

#[test]
fn the_smallcaps_class_is_a_base_class_in_the_authors_slot() {
    // PART 10 §1: the mandatory base class goes INSIDE the author's class slot,
    // so an id written before any class keeps its position.
    let html = carve::render_html(&decoded(WITH_ATTRS)).expect("renders");
    assert!(
        html.contains("<span id=\"agency\" class=\"smallcaps loud\">nasa</span>"),
        "{html}"
    );
}

#[test]
fn markdown_uses_the_same_inline_html() {
    // PART 11 §8c: the node has no Markdown delimiter spelling.
    let markdown = carve::render_markdown(&decoded(BARE)).expect("renders");
    assert!(
        markdown.contains("<span class=\"smallcaps\">nasa</span>"),
        "{markdown}"
    );
}

#[test]
fn plain_and_ansi_keep_the_children_and_their_letter_case() {
    let plain = carve::render_plain_text(&decoded(BARE)).expect("renders");
    assert_eq!(plain.trim(), "nasa");
    let ansi = carve::render_ansi(&decoded(BARE)).expect("renders");
    assert!(ansi.contains("nasa"), "{ansi}");
    assert!(!ansi.contains("NASA"), "the case is the author's: {ansi}");
}

#[test]
fn the_canonical_writer_drops_the_wrapper_and_keeps_the_children() {
    let written = carve::render_carve(&decoded(BARE)).expect("the writer spells the children");
    assert_eq!(written.trim(), "nasa");
}

#[test]
fn the_canonical_writer_keeps_attributes_on_an_ordinary_span() {
    let written = carve::render_carve(&decoded(WITH_ATTRS)).expect("the writer spells the span");
    assert_eq!(written.trim(), "[nasa]{#agency .loud}");
    // And what it wrote is an ordinary attributed span, not a second small_caps:
    // the round trip below is where inventing a source spelling would show.
    let reparsed = carve::parse(&written);
    let InlineNode::Span(span) = first_inline(&reparsed) else {
        panic!("it reads back as a span: {written}");
    };
    assert_eq!(span.children.len(), 1);
}

#[test]
fn an_empty_wrapper_is_not_an_unspellable_mark() {
    // An empty `underline` has no spelling and the writer refuses it. An empty
    // small_caps needs no spelling at all, so it leaves nothing behind instead.
    let empty = r#"{"type":"small_caps","children":[]}"#;
    assert_eq!(
        carve::render_carve(&decoded(empty))
            .expect("the writer does not refuse it")
            .trim(),
        ""
    );
    let empty_with_attrs =
        r#"{"type":"small_caps","children":[],"attrs":{"classes":["x"],"order":[".class"]}}"#;
    assert_eq!(
        carve::render_carve(&decoded(empty_with_attrs))
            .expect("nor this one")
            .trim(),
        "[]{.x}"
    );
}

#[test]
fn no_carve_source_produces_one() {
    // Every spelling a reader might reach for: the braced marks, a class, and
    // the HTML name. None of them is a small_caps.
    for source in [
        "{=nasa=}\n",
        "/nasa/\n",
        "[nasa]{.smallcaps}\n",
        "<span class=\"smallcaps\">nasa</span>\n",
    ] {
        let document = carve::parse(source);
        let json = carve::to_json(&document);
        assert!(
            !json.contains("small_caps"),
            "{source:?} must not parse to a small_caps: {json}"
        );
    }
}

#[test]
fn a_profile_can_deny_it_by_name() {
    let profile = Profile::default()
        .deny_inline(&["small_caps"])
        .on_disallowed(DisallowedAction::Strip);
    assert!(!profile.is_type_allowed("small_caps"));
    let filtered = apply_profile(decoded(BARE), &profile, None).expect("strip does not error");
    let json = carve::to_json(&filtered.doc);
    assert!(!json.contains("small_caps"), "{json}");
}

/// The bridge carries the wrapper as a MARK (carve-grammars#562, taken in
/// carve-rs#1876), so the letters stay text and stay editable.
///
/// Before that the map named nothing for it and the wrapper was reported lost
/// while its children went through unmarked. A mark also means the trip is
/// exact, which a degrade never was.
#[test]
fn the_prosemirror_bridge_carries_the_wrapper_as_a_mark() {
    let document = decoded(BARE);
    let pm = carve::to_prosemirror(&document);
    assert!(pm.json.contains("nasa"), "{}", pm.json);
    assert!(
        pm.degraded.is_empty() && pm.dropped.is_empty(),
        "an unattributed wrapper loses nothing: {:?} / {:?}",
        pm.degraded,
        pm.dropped
    );
    let back = carve::from_prosemirror(&pm.json).expect("what it wrote reads back");
    assert!(matches!(
        first_inline(&back),
        InlineNode::Emphasis(e) if e.kind == EmphasisKind::SmallCaps
    ));
    assert_eq!(carve::to_json(&back), carve::to_json(&document));
}

/// An ATTRIBUTED wrapper puts its run on an ordinary span and says so.
///
/// CARVE-P12-050 asks a canonical writer to keep the run on an attributed span
/// around the same children rather than on the wrapper, and the map's entry says
/// the mark carries the small-caps distinction alone. Nothing is lost, but the
/// wrapper comes back nested inside a span instead of carrying the run, and that
/// is a shape change a consumer can see.
#[test]
fn an_attributed_wrapper_moves_its_run_to_a_span_and_reports_the_nesting() {
    let pm = carve::to_prosemirror(&decoded(WITH_ATTRS));
    assert!(
        pm.degraded.contains_key("small_caps"),
        "the reshaping is reported: {:?}",
        pm.degraded
    );
    let back = carve::from_prosemirror(&pm.json).expect("what it wrote reads back");
    let InlineNode::Span(span) = first_inline(&back) else {
        panic!("expected the run on a span, got {:?}", first_inline(&back));
    };
    let attrs = span.attrs.as_ref().expect("the run survives");
    assert_eq!(attrs.id.as_deref(), Some("agency"));
    assert_eq!(attrs.classes, vec!["loud".to_string()]);
    assert!(matches!(
        &span.children[0],
        InlineNode::Emphasis(e) if e.kind == EmphasisKind::SmallCaps
    ));
}
