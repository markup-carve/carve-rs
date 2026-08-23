//! PART 11 §1 on the two readings that broke it: `parse(fmt(x)) == parse(x)`
//! (markup-carve/carve-rs#1277, ruling markup-carve/carve#1602).
//!
//! The corpus sweep in `render_carve.rs` covers these over 1370 documents, but
//! it covers only the SPELLINGS THE CORPUS HAPPENS TO HOLD. Both readings here
//! have more than one, and one of the extra ones is in no corpus document at
//! all: an item whose whole body is a LINE BLOCK (`::: |`) broke exactly as the
//! admonition did, and only a written-out sweep of the sibling fence kinds
//! found it. So each spelling gets a row, and the fence kinds that must NOT
//! move get rows too - a fix with no boundary is indistinguishable from a fix
//! that was applied too widely.

/// The canonical writer's output must parse to the tree its input did.
///
/// Compared through this crate's own AST JSON with `pos` dropped. The escaping
/// and text-run allowances the corpus sweep makes are not needed here: none of
/// these documents gains or moves an escape, so the raw trees compare.
fn parses_the_same(source: &str) -> bool {
    fn tree(source: &str) -> String {
        fn strip(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(fields) => {
                    fields.remove("pos");
                    fields.remove("srcByteLength");
                    for (_, child) in fields.iter_mut() {
                        strip(child);
                    }
                }
                serde_json::Value::Array(items) => items.iter_mut().for_each(strip),
                _ => {}
            }
        }
        let mut value: serde_json::Value =
            serde_json::from_str(&carve::to_json(&carve::parse(source))).expect("own AST JSON");
        strip(&mut value);
        value.to_string()
    }
    tree(source) == tree(&carve::to_carve(source))
}

/// Whether each list in the document is tight, in document order.
fn tightness(source: &str) -> Vec<bool> {
    carve::parse(source)
        .children
        .iter()
        .filter_map(|block| match block {
            carve::BlockNode::List(list) => Some(list.tight),
            _ => None,
        })
        .collect()
}

// ------------------------------------------------- the emptied item, `- +`

/// An item whose only content was a collected definition has NO children.
///
/// The writer spells such an item `- +`, so this is the engine reading its own
/// output. It built a paragraph with no children there - a node the author's
/// document did not contain - while the STANDALONE `+` line, the other
/// spelling of the same §17 construct, already filtered it out. carve-js and
/// carve-php build no node in either spelling.
#[test]
fn an_emptied_item_holds_no_block_in_either_continuation_spelling() {
    fn last_item_children(source: &str) -> Vec<carve::BlockNode> {
        let doc = carve::parse(source);
        let carve::BlockNode::List(list) = &doc.children[0] else {
            panic!("the document opens with a list: {source:?}");
        };
        list.items
            .last()
            .expect("the list has an item")
            .children
            .clone()
    }
    fn holds_an_empty_paragraph(children: &[carve::BlockNode]) -> bool {
        children
            .iter()
            .any(|block| matches!(block, carve::BlockNode::Paragraph(p) if p.children.is_empty()))
    }

    // THE MARKER-LINE SPELLING, which is what `fmt` emits for an emptied item.
    // The item has no content at all, so it has no children at all.
    let marker_line = last_item_children("- +\n\nSee [it][ref].\n\n[ref]: /url\n");
    assert!(
        marker_line.is_empty(),
        "an emptied item gained a block: {marker_line:?}"
    );

    // THE STANDALONE SPELLING of the same §17 construct. Here the item has a
    // lead paragraph, so what must not appear is the EXTRA empty paragraph the
    // emptied attachment would leave - this spelling already filtered it, and
    // the marker-line one did not.
    let standalone = last_item_children("- a\n+\n[ref]: /url\n\nSee [it][ref].\n");
    assert!(
        !holds_an_empty_paragraph(&standalone),
        "the attached emptied block left a node: {standalone:?}"
    );
    assert_eq!(
        standalone.len(),
        1,
        "only the lead paragraph: {standalone:?}"
    );
}

#[test]
fn the_emptied_item_survives_its_own_round_trip() {
    let source = "- [ref]: /url\n\nSee [it][ref].\n";
    assert_eq!(
        carve::to_carve(source),
        "- +\n\nSee [it][ref].\n\n[ref]: /url\n"
    );
    assert!(parses_the_same(source), "parse(fmt(x)) != parse(x)");
}

// ------------------------------- the container that is the item's whole body

/// Supplying a missing closer must not move the list's tightness.
///
/// `fmt` closes an unterminated container, so the two spellings below are `x`
/// and `fmt(x)`. The trees are otherwise identical - the item holds one
/// container either way and `tail` sits inside it - so only the flag moved,
/// which is what markup-carve/carve#1602 ruled out. Both read LOOSE, the
/// reading the source already gave and the one carve-php gives both.
#[test]
fn a_supplied_closer_does_not_tighten_an_item_whose_body_is_the_container() {
    for (kind, unterminated, terminated) in [
        (
            "admonition",
            "- ::: d\n  b\n\n  tail\n",
            "- ::: d\n  b\n\n  tail\n  :::\n",
        ),
        // IN NO CORPUS DOCUMENT. Found only by sweeping the sibling fence kinds
        // after the admonition was fixed.
        (
            "line block",
            "- ::: |\n  b\n\n  tail\n",
            "- ::: |\n  b\n\n  tail\n  :::\n",
        ),
    ] {
        assert_eq!(
            carve::to_carve(unterminated),
            terminated,
            "{kind}: fmt output"
        );
        assert_eq!(tightness(unterminated), vec![false], "{kind}: unterminated");
        assert_eq!(tightness(terminated), vec![false], "{kind}: terminated");
        assert!(
            parses_the_same(unterminated),
            "{kind}: parse(fmt(x)) != parse(x)"
        );
    }
}

/// The bound, from the other side: a container with a block BESIDE it in the
/// item is one block among several, and its interior blank stays its own
/// content (markup-carve/carve#985). Corpus
/// `279-a-boundary-line-inside-an-open-fence-does-not-end-the-container-10`
/// reaches the looseness scan with the same lines as the document above, so
/// only the caller can tell the two apart - lifting the skip for it instead
/// flips this to loose and moves the corpus HTML.
#[test]
fn a_container_with_a_block_beside_it_keeps_the_item_tight() {
    let source = "- x\n  :::\n  a\n\n  b\n  :::\n";
    assert_eq!(tightness(source), vec![true]);
    assert!(parses_the_same(source), "parse(fmt(x)) != parse(x)");
}

/// And the other bound: a CODE fence body is verbatim, and an item whose whole
/// body is one never had this defect - it reads tight on both sides already
/// (`fence_interior_blank_looseness.rs` pins the tightness itself). Widening
/// the lift to the verbatim fences would have moved it for no reason.
#[test]
fn a_verbatim_fence_body_is_untouched() {
    for source in [
        "- ```\n  b\n\n  tail\n",
        "- ```\n  b\n\n  tail\n  ```\n",
        "- ```=html\n  b\n\n  tail\n",
    ] {
        assert_eq!(tightness(source), vec![true], "{source:?}");
        assert!(
            parses_the_same(source),
            "parse(fmt(x)) != parse(x) for {source:?}"
        );
    }
}
