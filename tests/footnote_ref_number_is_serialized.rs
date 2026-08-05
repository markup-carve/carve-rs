//! `footnote_ref.number` reaches the wire.
//!
//! PART 12 §5 names it as a resolution result that IS serialized, "because
//! recomputing them requires reimplementing PART 9R". The number was computed -
//! the HTML numbers the notes correctly - but only inside the HTML renderer, so
//! `--ast` and every other consumer of the tree saw none (carve-rs#638).
//!
//! Caption numbers were already right, which is the tell: they are assigned in
//! `parse` under the HTML mode, where the AST path also runs, while footnote
//! numbering lived in `render`.
//!
//! `ref_id` is deliberately NOT assigned on this path. It is an HTML backlink
//! anchor; the schema permits the field and carve-js does not publish it, so
//! writing it here would put something on the wire no other engine emits. The
//! numbering pass takes a flag rather than being followed by a second sweep -
//! this tree has 51 inline variants and a hand-written walk to undo one field
//! would silently miss one.

use carve::ast::{BlockNode, InlineNode};

/// Every footnote reference in the published tree, as (id, number, has_ref_id).
fn refs(src: &str) -> Vec<(Option<String>, Option<usize>, bool)> {
    let doc = carve::parse(src);
    let mut out = Vec::new();
    for block in &doc.children {
        if let BlockNode::Paragraph(p) = block {
            for inline in &p.children {
                if let InlineNode::Footnote(f) = inline {
                    out.push((f.id.clone(), f.number, f.ref_id.is_some()));
                }
            }
        }
    }
    out
}

#[test]
fn a_reference_carries_its_number() {
    let found = refs("[^a]: note\n\nsee[^a] again[^a]\n");
    assert_eq!(found.len(), 2, "expected two references, got {found:?}");
    // A REPEAT reuses the number rather than taking the next one.
    assert_eq!(found[0].1, Some(1), "{found:?}");
    assert_eq!(found[1].1, Some(1), "{found:?}");
}

#[test]
fn two_notes_number_in_first_use_order() {
    let found = refs("[^b]: second\n[^a]: first\n\nsee[^a] then[^b]\n");
    // Order of USE, not of definition - `[^a]` is referenced first.
    assert_eq!(found[0].1, Some(1), "{found:?}");
    assert_eq!(found[1].1, Some(2), "{found:?}");
}

#[test]
fn a_reference_to_no_definition_gets_no_number() {
    // The gate the numbering pass applies: an undefined label is not a footnote,
    // so numbering it would invent one.
    let found = refs("see[^missing]\n");
    assert!(
        found.iter().all(|(_, number, _)| number.is_none()),
        "{found:?}"
    );
}

#[test]
fn the_published_tree_does_not_carry_ref_id() {
    // carve-js publishes `number` and not `refId`; the schema allows either, so
    // matching it is a deliberate choice rather than a constraint.
    let found = refs("[^a]: note\n\nsee[^a] again[^a]\n");
    assert!(
        found.iter().all(|(_, _, has_ref_id)| !has_ref_id),
        "refId reached the published tree: {found:?}"
    );
}

#[test]
fn the_html_still_numbers_and_still_backlinks() {
    // The other half of the flag: the HTML path keeps assigning `ref_id`, so the
    // backlink anchors are unchanged. Without this, "do not publish refId" could
    // be satisfied by never computing it.
    let html = carve::to_html("[^a]: note\n\nsee[^a] again[^a]\n");
    assert!(html.contains("fnref1"), "{html}");
    assert!(html.contains("fnref1-2"), "{html}");
    assert!(html.contains("id=\"fn1\""), "{html}");
}

#[test]
fn caption_numbers_still_reach_the_wire() {
    // Already correct before this change, and the neighbouring pass - pinned so a
    // change to where footnote numbering runs cannot disturb it.
    let doc = carve::parse("![a](/p.png)\n^ Figure #: cap\n");
    let json = format!("{doc:?}");
    assert!(json.contains("CaptionNumber"), "{json}");
}
