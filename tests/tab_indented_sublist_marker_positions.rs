//! A tab-indented sublist marker is placed, and its span names the marker.
//!
//! The second site the unsigned column map lost, found by mutating the first.
//! `slice_columns_mapped` re-emits the columns a straddling tab overshot as
//! SPACES, and the list-item collector recorded `consumed - synthetic` for the
//! line. Where the slice wrote MORE than it consumed - `- a` over `<TAB>- b`,
//! which is one character consumed and two written - that difference is `-1`,
//! and the collector answered `None` for the line rather than hold a negative
//! constant (the `checked_sub` added by markup-carve/carve-rs#700).
//!
//! Everything anchored on that line then went unplaced: the sublist, its item,
//! its paragraph and its text - four positions on a two-line document. Neither
//! the spec corpus nor any test reached the shape, which is why the signed map
//! (markup-carve/carve-rs#736) is what surfaced it.
//!
//! The rendered document is unaffected either way; only the positions moved.

use carve::{BlockNode, InlineNode, Options};

/// `- a` over a TAB, then `- b`: the sublist marker sits at source offset 5.
const TAB_SUBLIST: &str = "- a\n\t- b\n";

/// Every `(type, startOffset, endOffset)` in the tree, in walk order.
fn placed(source: &str) -> Vec<(&'static str, Option<(usize, usize)>)> {
    let doc = carve::parse_with_options(source, &Options::default().with_positions(true));
    let mut out = Vec::new();
    fn blocks(nodes: &[BlockNode], out: &mut Vec<(&'static str, Option<(usize, usize)>)>) {
        for node in nodes {
            match node {
                BlockNode::List(l) => {
                    out.push((
                        "list",
                        l.pos.as_ref().map(|p| (p.start_offset, p.end_offset)),
                    ));
                    for item in &l.items {
                        out.push((
                            "list_item",
                            item.pos.as_ref().map(|p| (p.start_offset, p.end_offset)),
                        ));
                        blocks(&item.children, out);
                    }
                }
                BlockNode::Paragraph(p) => {
                    out.push((
                        "paragraph",
                        p.pos.as_ref().map(|x| (x.start_offset, x.end_offset)),
                    ));
                    for inline in &p.children {
                        if let InlineNode::Text(t) = inline {
                            out.push((
                                "text",
                                t.pos.as_ref().map(|x| (x.start_offset, x.end_offset)),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    blocks(&doc.children, &mut out);
    out
}

#[test]
fn the_sublist_under_a_tab_is_placed() {
    let found = placed(TAB_SUBLIST);
    let unplaced: Vec<_> = found.iter().filter(|(_, p)| p.is_none()).collect();
    assert!(
        unplaced.is_empty(),
        "these nodes carry no position: {unplaced:?}\nall: {found:?}"
    );
}

#[test]
fn the_sublist_span_names_the_marker_the_author_wrote() {
    // Stronger than "present". Offsets in `- a\n\t- b\n`: the tab is 4, the
    // sublist marker 5, and `b` 7.
    let chars: Vec<char> = TAB_SUBLIST.chars().collect();
    let found = placed(TAB_SUBLIST);
    let inner_list = found
        .iter()
        .filter(|(ty, _)| *ty == "list")
        .nth(1)
        .expect("a nested list");
    let (start, end) = inner_list.1.expect("the nested list is placed");
    assert_eq!(
        chars[start..end].iter().collect::<String>(),
        "- b",
        "the nested list's span names the wrong source"
    );
    let inner_text = found
        .iter()
        .filter(|(ty, _)| *ty == "text")
        .nth(1)
        .expect("a nested text node");
    let (ts, te) = inner_text.1.expect("the nested text is placed");
    assert_eq!(chars[ts..te].iter().collect::<String>(), "b");
}

#[test]
fn an_ordered_tab_indented_sublist_is_placed_too() {
    // Same shape, different marker: the constant is the line's, not the
    // marker's, so nothing about it should be bullet-specific.
    let found = placed("1. a\n\t\t1. c\n");
    let unplaced: Vec<_> = found.iter().filter(|(_, p)| p.is_none()).collect();
    assert!(
        unplaced.is_empty(),
        "these nodes carry no position: {unplaced:?}\nall: {found:?}"
    );
}

#[test]
fn the_space_indented_sublist_is_unchanged() {
    // The control: with no tab nothing is synthesized, so none of this applies
    // and the shape was always placed.
    let source = "- a\n    - b\n";
    let chars: Vec<char> = source.chars().collect();
    let found = placed(source);
    assert!(
        found.iter().all(|(_, p)| p.is_some()),
        "a space-indented sublist lost a position: {found:?}"
    );
    let (start, end) = found
        .iter()
        .filter(|(ty, _)| *ty == "list")
        .nth(1)
        .and_then(|(_, p)| *p)
        .expect("a placed nested list");
    assert_eq!(chars[start..end].iter().collect::<String>(), "- b");
}

#[test]
fn the_rendering_is_the_same_either_way() {
    // The residual exists so two markers written at one visual column arrive at
    // one column. Nothing here may move that.
    let html = carve::to_html(TAB_SUBLIST);
    assert_eq!(
        html.matches("<ul>").count(),
        2,
        "expected an outer and a nested list:\n{html}"
    );
}
