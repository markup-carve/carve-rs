//! `citation.mode` is a fact of the ITEM, and the group's `mode` is its
//! summary (carve-rs#1858).
//!
//! The schema at `e2b7087f` names `mode` on both `citation` and
//! `citation_group`, and this engine only ever stored the summary: the field
//! was in `WIRE_FIELDS`, so a tree carrying it decoded cleanly, and
//! `decode_citation` never read it. Accepted and dropped without a word, which
//! PART 12 §9(b) calls the worst of the three outcomes.
//!
//! The shape the item-level field exists for is a group whose items DISAGREE.
//! No Carve source spells one - `[+@a; @b]` marks every item at once - but an
//! importer reaches it, and carve-js publishes it. A group flag cannot hold it,
//! so the round trip below is the whole point of the field: if a mixed group
//! comes out the way it went in, the fix is complete, and if it does not, no
//! amount of group-level bookkeeping closes the gap.

use carve::{Citations, InlineNode, Options};

/// A group whose two items disagree: `a` is integral, `b` is not.
const MIXED: &str = r#"{"type":"document","srcByteLength":8,"children":[
    {"type":"paragraph","children":[{"type":"citation_group","raw":"[@a; @b]","items":[
        {"type":"citation","key":"a","suppressAuthor":false,"mode":"integral"},
        {"type":"citation","key":"b","suppressAuthor":false}]}]}]}"#;

/// The shape every tree this engine wrote before the fix has: the summary on
/// the group and nothing on the items.
const SUMMARY_ONLY: &str = r#"{"type":"document","srcByteLength":8,"children":[
    {"type":"paragraph","children":[{"type":"citation_group","raw":"[+@a; @b]","mode":"integral","items":[
        {"type":"citation","key":"a","suppressAuthor":false},
        {"type":"citation","key":"b","suppressAuthor":false}]}]}]}"#;

const WRAPPER: &str = "<span class=\"citation\" data-cite-mode=\"integral\">";

fn citations() -> Citations {
    Citations::new()
}

fn first_group(doc: &carve::Document) -> carve::CitationGroup {
    let carve::BlockNode::Paragraph(p) = &doc.children[0] else {
        panic!("the first block is a paragraph");
    };
    p.children
        .iter()
        .find_map(|node| match node {
            InlineNode::CitationGroup(group) => Some(group.clone()),
            _ => None,
        })
        .expect("the paragraph holds a citation group")
}

fn modes(group: &carve::CitationGroup) -> Vec<Option<carve::CitationItemMode>> {
    group.items.iter().map(|item| item.mode).collect()
}

fn parse(source: &str) -> carve::Document {
    let citations = citations();
    carve::parse_with_options(source, &Options::new().with_extension(&citations))
}

/// Render `[+@a; @b]` with `drop` items' modes cleared, so the mixed case and
/// the all-integral control differ in exactly one thing: whether the second
/// item still carries its mode.
///
/// The group is built by parsing rather than decoded, because the renderer is
/// what is under test here and an ingested tree takes a different route to its
/// definitions.
fn render_with_modes_cleared(drop: &[usize]) -> String {
    let citations = citations();
    let options = Options::new().with_extension(&citations);
    let mut doc = carve::parse_with_options("[+@a; @b] here.\n\n[@a]: A.\n\n[@b]: B.\n", &options);
    let carve::BlockNode::Paragraph(p) = &mut doc.children[0] else {
        panic!("the first block is a paragraph");
    };
    for node in &mut p.children {
        if let InlineNode::CitationGroup(group) = node {
            for index in drop {
                group.items[*index].mode = None;
            }
        }
    }
    let doc = carve::prepare_document_for_render(doc, &options, carve::Mode::Interactive, true)
        .expect("no profile is configured, so nothing can be refused");
    carve::render_html_with_options(&doc, &options)
        .expect("the tree under test is within the render ceiling")
        .trim()
        .to_string()
}

/// THE CASE THE FIELD EXISTS FOR. A group whose items disagree comes out the
/// way it went in, through the AST and back onto the wire.
#[test]
fn a_group_whose_items_disagree_round_trips_intact() {
    let decoded = carve::from_json(MIXED).expect("the mixed group decodes");
    assert_eq!(
        modes(&first_group(&decoded)),
        vec![Some(carve::CitationItemMode::Integral), None],
        "decode kept one item's mode and not the other's"
    );

    let republished = carve::to_json(&decoded);
    let again = carve::from_json(&republished).expect("the republished tree decodes");
    assert_eq!(
        modes(&first_group(&again)),
        vec![Some(carve::CitationItemMode::Integral), None],
        "the mixed group did not survive the round trip: {republished}"
    );

    // And the summary tells the truth about it: the items disagree, so the
    // group is not integral and must not claim to be.
    assert!(!first_group(&again).integral());
    assert_eq!(
        republished.matches("\"mode\":\"integral\"").count(),
        1,
        "exactly one `mode` rides the wire - the item's: {republished}"
    );
}

/// A mixed group renders its marking on the item that carries it, which is the
/// only way the two halves can look different. carve-js does the same.
#[test]
fn a_mixed_group_marks_the_item_and_not_the_group() {
    let out = render_with_modes_cleared(&[1]);
    assert_eq!(
        out.matches(WRAPPER).count(),
        1,
        "one item is integral, so one wrapper: {out}"
    );
    assert!(
        out.contains(&format!("[{WRAPPER}")),
        "the wrapper sits INSIDE the brackets, around the item: {out}"
    );
}

/// The control: when every item agrees, the wrapper goes around the group, as
/// it always has.
#[test]
fn an_all_integral_group_still_wraps_the_whole_group() {
    let out = render_with_modes_cleared(&[]);
    assert_eq!(out.matches(WRAPPER).count(), 1, "{out}");
    assert!(
        out.contains(&format!("{WRAPPER}[")),
        "the wrapper sits OUTSIDE the brackets, around the group: {out}"
    );
}

/// Source spells the mode once, for the whole group, so every item carries it
/// and the summary derives.
#[test]
fn a_source_spelled_group_marks_every_item() {
    let integral = first_group(&parse("[+@a; @b]\n"));
    assert_eq!(
        modes(&integral),
        vec![
            Some(carve::CitationItemMode::Integral),
            Some(carve::CitationItemMode::Integral)
        ]
    );
    assert!(integral.integral());

    let plain = first_group(&parse("[@a; @b]\n"));
    assert_eq!(modes(&plain), vec![None, None]);
    assert!(!plain.integral());
}

/// PUBLISHED, not only read: every item of a source-spelled group spells its
/// own mode on the wire, which is what carve-js sees.
#[test]
fn every_item_of_a_published_group_carries_its_mode() {
    let published = carve::to_json(&parse("[+@a; @b]\n"));
    assert_eq!(
        published.matches("\"mode\":\"integral\"").count(),
        3,
        "two items and the group's summary: {published}"
    );
}

/// A tree that carries only the summary - every tree this engine wrote before
/// the fix - keeps its marking. The schema calls the group flag the shorthand
/// every item of a source-spelled group carries, so reading it onto the items
/// restores a fact rather than inventing one.
#[test]
fn a_tree_that_carries_only_the_summary_keeps_its_marking() {
    let decoded = carve::from_json(SUMMARY_ONLY).expect("the summary-only group decodes");
    let group = first_group(&decoded);
    assert_eq!(
        modes(&group),
        vec![
            Some(carve::CitationItemMode::Integral),
            Some(carve::CitationItemMode::Integral)
        ]
    );
    assert!(group.integral());
    assert!(
        carve::to_json(&decoded).contains("\"mode\":\"integral\""),
        "the marking must not be dropped on republish"
    );
}
