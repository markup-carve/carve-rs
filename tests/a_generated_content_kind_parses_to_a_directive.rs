//! CARVE-P12-057: `:::` produces one of three types. An anonymous or
//! attribute-only container is a `div`. A named one whose kind names GENERATED
//! CONTENT is a `directive`. Every other named container is an `admonition`.
//!
//! The kind list is CLOSED, so a seventh generated-looking word is an
//! admonition. That is what most of this file pins; the anonymous and
//! attribute-only cases are the control.

use carve::ast::{BlockNode, Document, GENERATED_CONTENT_KINDS};

fn parse(src: &str) -> Document {
    carve::parse(src)
}

fn only_block(doc: &Document) -> &BlockNode {
    assert_eq!(doc.children.len(), 1, "one block: {:?}", doc.children);
    &doc.children[0]
}

fn type_of(node: &BlockNode) -> &'static str {
    match node {
        BlockNode::Directive(_) => "directive",
        BlockNode::Admonition(_) => "admonition",
        BlockNode::Div(_) => "div",
        other => panic!("unexpected block: {other:?}"),
    }
}

#[test]
fn each_of_the_six_kinds_is_a_directive_carrying_that_kind() {
    assert_eq!(
        GENERATED_CONTENT_KINDS,
        [
            "bibliography",
            "footnotes",
            "glossary",
            "index",
            "references",
            "toc"
        ],
        "the clause names these six"
    );
    for kind in GENERATED_CONTENT_KINDS {
        let doc = parse(&format!("::: {kind}\nbody\n:::\n"));
        let BlockNode::Directive(d) = only_block(&doc) else {
            panic!("{kind} is a {}", type_of(only_block(&doc)));
        };
        assert_eq!(d.kind, *kind);
        assert_eq!(d.children.len(), 1, "the authored body is carried");
    }
}

#[test]
fn the_list_is_closed_so_a_generated_looking_word_is_an_admonition() {
    // Not "unknown, so guess": the clause rules that every other named
    // container is an admonition, which makes the six a closed set rather than
    // a prefix of one.
    for kind in [
        "endnotes",
        "contents",
        "bibliographies",
        "toc-2",
        "Toc",
        "note",
        "warning",
        "sidebar",
    ] {
        let doc = parse(&format!("::: {kind}\nbody\n:::\n"));
        assert_eq!(
            type_of(only_block(&doc)),
            "admonition",
            "::: {kind} is not one of the six"
        );
    }
}

#[test]
fn an_anonymous_or_attribute_only_container_stays_a_div() {
    // THE CONTROL. Splitting the named branch must not move either of these.
    // An attribute block goes on the line ABOVE the opener; Carve has no
    // trailing attribute block on a fence line.
    for src in [
        ":::\nbody\n:::\n",
        "{.box}\n:::\nbody\n:::\n",
        "{#a .b}\n:::\nbody\n:::\n",
    ] {
        let doc = parse(src);
        assert_eq!(type_of(only_block(&doc)), "div", "{src:?} is anonymous");
    }
}

#[test]
fn a_directive_carries_its_opener_label() {
    let doc = parse("::: toc [First]\n:::\n");
    let BlockNode::Directive(d) = only_block(&doc) else {
        panic!("expected a directive");
    };
    assert_eq!(d.label.as_deref(), Some("First"));
}

#[test]
fn a_directive_publishes_no_title_because_the_schema_names_none() {
    // markup-carve/carve#2247: `:::` admits a quoted title on every named
    // container and `directive` has no slot for one, so the title is not
    // carried. Pinned rather than left to be discovered.
    let json = carve::to_json(&parse("::: toc \"Contents\"\n:::\n"));
    assert!(
        json.contains(r#""type":"directive""#),
        "still a directive: {json}"
    );
    assert!(
        !json.contains("Contents"),
        "the opener title has nowhere to go on this node: {json}"
    );
}

#[test]
fn it_survives_the_ast_json_round_trip_as_a_directive() {
    let doc = parse("::: glossary [G]\nbody\n:::\n");
    let written = carve::to_json(&doc);
    let decoded = carve::from_json(&written).expect("decodable");
    assert_eq!(type_of(only_block(&decoded)), "directive");
    assert_eq!(carve::to_json(&decoded), written, "byte-identical");
}

#[test]
fn the_html_is_the_same_generic_div_the_kind_always_rendered() {
    // A directive's kind is never Tier-1, so it takes the same
    // `<div class="{kind}">` shape the non-canonical admonition it replaced took.
    // That is why no golden moved.
    let html = carve::render_html(&parse("::: toc\nbody\n:::\n")).expect("renderable");
    assert_eq!(html.trim(), "<div class=\"toc\">\n  <p>body</p>\n</div>");
    let with_attrs =
        carve::render_html(&parse("{#i .wide}\n::: index\nbody\n:::\n")).expect("renderable");
    assert!(
        with_attrs.contains("<div class=\"index wide\" id=\"i\">"),
        "the attribute block above the opener still merges: {with_attrs}"
    );
}

#[test]
fn the_canonical_writer_spells_it_back_to_the_same_source() {
    for src in [
        "::: toc\n:::\n",
        "::: references [R]\n:::\n",
        "::: index\nbody\n:::\n",
    ] {
        let doc = parse(src);
        let written = carve::render_carve(&doc).expect("spellable");
        assert_eq!(
            carve::parse(&written).children,
            doc.children,
            "parse(fmt(x)) == parse(x) for {src:?}, written as {written:?}"
        );
    }
}

#[test]
fn a_profile_that_denies_div_still_strips_it() {
    // Splitting the type must not WIDEN a deny list: `::: toc` classified as
    // `div` before the split, through the non-Tier-1-kind branch, so a profile
    // denying `div` kept stripping it. `with_supertype` is what keeps that true.
    let doc = parse("::: toc\nbody\n:::\n");
    assert_eq!(
        carve::profile::canonical_block_type(only_block(&doc)),
        Some("directive"),
        "it classifies under its own name"
    );
    for denied in ["div", "directive"] {
        let profile = carve::Profile::full().deny_block(&[denied]);
        let filtered = carve::apply_profile(parse("::: toc\nbody\n:::\n"), &profile, None)
            .expect("the default action strips rather than errors");
        assert!(
            !filtered
                .doc
                .children
                .iter()
                .any(|child| matches!(child, BlockNode::Directive(_))),
            "denying {denied:?} strips it: {:?}",
            filtered.doc.children
        );
    }
}

#[test]
fn the_footnotes_marker_still_places_the_endnotes_section_where_it_was_written() {
    let html = carve::render_html(&parse("a[^1]\n\n::: footnotes\n:::\n\nafter\n\n[^1]: n\n"))
        .expect("renderable");
    let section = html
        .find("doc-endnotes")
        .expect("the endnotes section renders");
    let after = html.find("after").expect("the later paragraph renders");
    assert!(
        section < after,
        "the section is flushed at the marker, not at document end: {html}"
    );
}

#[test]
fn the_placement_extension_still_fills_it() {
    // `TocPlacement`, not `TableOfContents`: the placement extension is the one
    // that CONSUMES the container, so it is the one whose keying moved. The
    // injecting extension emits its nav whether or not a container exists, and
    // would pass this either way - it did, under the mutation below.
    let toc = carve::TocPlacement::new();
    let options = carve::Options::new().with_extension(&toc);
    let html = carve::to_html_with_options("::: toc\n:::\n\n# One\n\n# Two\n", &options);
    assert!(
        html.contains("<nav class=\"toc\"") && html.contains("#One"),
        "the extension keys on the directive now: {html}"
    );
}
