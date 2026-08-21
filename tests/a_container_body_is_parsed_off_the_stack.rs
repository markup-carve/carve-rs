//! A colon container's body is parsed by a WORKLIST, not by recursive descent.
//!
//! `collect_colon_container_body` materializes a body before anything parses
//! it, so the level that opened the container has nothing half-finished to
//! suspend. `parse_blocks` therefore emits the container node with EMPTY
//! children, records the body, and runs to the end of its own input;
//! `resolve_pending_containers` parses the bodies afterwards and stitches them
//! back in reverse discovery order (markup-carve/carve-rs#1165).
//!
//! The stack floor that buys is asserted by `stack_floor_attribution.rs`. What
//! is asserted HERE is that the tree is the same tree - because the two ways
//! this can go wrong are both silent:
//!
//! - a body stitched into the WRONG node, which needs siblings and nesting in
//!   the same document to show up at all; and
//! - state the recursive form carried in an RAII guard and the worklist has to
//!   carry in the work item instead - the nesting depth, and whether the body
//!   is inside an open figure group.
//!
//! The full corpus covers containers broadly but shallowly; nothing in it nests
//! to the cap, which is exactly the shape the conversion is for.

use carve::ast::BlockNode;

/// A ladder of `depth` flush-left containers, properly closed, with one
/// paragraph at the bottom. `::::` throughout, so every closer matches every
/// opener and the nesting is unambiguous.
fn ladder(depth: usize, opener: &str) -> String {
    format!(
        "{}deep\n{}",
        format!("{opener}\n").repeat(depth),
        "::::\n".repeat(depth)
    )
}

/// The children of a container node, whatever kind of container it is.
fn container_children(node: &BlockNode) -> Option<&Vec<BlockNode>> {
    match node {
        BlockNode::Admonition(n) => Some(&n.children),
        BlockNode::Div(n) => Some(&n.children),
        BlockNode::FigureGroup(n) => Some(&n.children),
        _ => None,
    }
}

/// How many containers deep the tree goes down its first container spine.
///
/// CONTAINERS ONLY: it does not descend through a quote or a list item, so a
/// container nested inside one reads as depth 0 here. The tests that put a
/// container inside those assert on the rendered shape instead.
///
/// Iterative, so the assertion does not reintroduce on the test's stack the
/// recursion it is checking the parser no longer does.
fn container_depth(children: &[BlockNode]) -> usize {
    let mut depth = 0;
    let mut level = children;
    while let Some(inner) = level.iter().find_map(container_children) {
        depth += 1;
        level = inner;
    }
    depth
}

const CAP: usize = 200;

#[test]
fn a_ladder_at_the_nesting_cap_keeps_every_level() {
    let doc = carve::parse(&ladder(CAP, ":::: note"));
    assert_eq!(
        container_depth(&doc.children),
        CAP,
        "a level was lost between the level loop and the stitch"
    );
    // The body reached the bottom rather than being stranded in a slot.
    assert!(carve::to_html(&ladder(CAP, ":::: note")).contains("deep"));
}

#[test]
fn a_ladder_at_the_nesting_cap_keeps_every_level_with_positions_on() {
    // The POSITIONED chain, which is a different set of helpers end to end:
    // the mapped-source path carries the container's line and column maps and
    // never reaches the unpositioned ones.
    let doc = carve::parse_with_options(
        &ladder(CAP, ":::: note"),
        &carve::Options::default().with_positions(true),
    );
    assert_eq!(container_depth(&doc.children), CAP);
}

#[test]
fn siblings_keep_their_source_order_and_their_own_bodies() {
    // The failure this rules out: bodies parsed out of order and stitched into
    // each other's nodes. Two containers with DIFFERENT kinds and different
    // bodies, so a swap is visible in both halves.
    let doc = carve::parse("::: note\nfirst\n:::\n\n::: warning\nsecond\n:::\n");
    let kinds: Vec<&str> = doc
        .children
        .iter()
        .filter_map(|node| match node {
            BlockNode::Admonition(n) => Some(n.kind.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, vec!["note", "warning"]);

    let html = carve::to_html("::: note\nfirst\n:::\n\n::: warning\nsecond\n:::\n");
    let first = html.find("first").expect("the first body renders");
    let second = html.find("second").expect("the second body renders");
    assert!(
        first < second,
        "the bodies were stitched in the wrong order"
    );
}

#[test]
fn a_nested_body_lands_in_the_node_that_opened_it() {
    // A container with a body of its own AND a nested container after it. If
    // the stitch pairs a slot with the wrong node, the inner text moves.
    let doc = carve::parse("::: note\nouter\n\n::: warning\ninner\n:::\n\n:::\n");
    let outer = doc
        .children
        .iter()
        .find_map(container_children)
        .expect("the outer container");
    let nested = outer
        .iter()
        .find_map(container_children)
        .expect("the nested container");
    assert_eq!(
        container_depth(&doc.children),
        2,
        "the nest should be exactly two containers deep"
    );
    assert!(
        nested
            .iter()
            .any(|node| matches!(node, BlockNode::Paragraph(_))),
        "the inner body did not reach the inner node"
    );
}

#[test]
fn a_group_body_still_demotes_the_bare_openers_inside_it() {
    // PART 9 §4c: groups do not nest. The recursive form said this with a
    // `FigureGroupGuard` held across the body parse; the worklist has to carry
    // the same fact in the work item, so a bare `::: figure` inside a group's
    // body stays a generic container.
    let doc = carve::parse("::: figure\n::: figure\ninner\n:::\n:::\n");
    let outer = doc.children.first().expect("the outer container");
    assert!(
        matches!(outer, BlockNode::FigureGroup(_)),
        "the outer bare opener is the composite figure"
    );
    let inner = container_children(outer)
        .expect("the group's body")
        .first()
        .expect("the inner container");
    assert!(
        matches!(inner, BlockNode::Admonition(_)),
        "a bare opener inside an open group is a generic container, not a second group"
    );
}

#[test]
fn a_container_inside_a_quote_and_inside_an_item_still_resolves() {
    // A quote and an item each parse their content through their OWN call, so
    // the container inside one is recorded on that call's pending list and
    // drained there - not on the document's. The rendered shape is asserted in
    // full, and it is the shape this repo produced before the conversion.
    assert_eq!(
        carve::to_html("> ::: note\n> inside\n> :::\n").trim(),
        "<blockquote>\n  <aside class=\"admonition note\" aria-label=\"Note\">\n    <p>inside</p>\n  \
         </aside>\n</blockquote>"
    );
    assert_eq!(
        carve::to_html("- ::: note\n  inside\n  :::\n").trim(),
        "<ul>\n  <li>\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>inside</p>\n    \
         </aside>\n  </li>\n</ul>"
    );
}

#[test]
fn a_block_attribute_still_attaches_to_the_container_it_precedes() {
    // The node is pushed hollow and the attribute is applied to it in the same
    // step, before the body exists - so this pins that the attribute reaches
    // the container rather than the body, and that the index recorded for the
    // pending body still points at the node after the attribute was applied.
    let html = carve::to_html("{#outer}\n::: note\nx\n:::\n");
    assert!(
        html.contains("id=\"outer\""),
        "the attribute did not reach the container: {html}"
    );
}
