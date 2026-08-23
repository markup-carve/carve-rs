//! PART 11 §10j: an unspellable block does not cancel the adjacency it cannot
//! spell.
//!
//! Where EVERY block between two sibling lists reaches the writer from the AST
//! and leaves no character on the page, the two lists are ADJACENT for §10i and
//! the PART 9 §11 N1a boundary is written anyway. Without it the lists come
//! back with one blank line between them, which is the loose separator, so they
//! MERGE - two lists become one and the items change shape with them.
//!
//! STATED OVER WHAT A BLOCK SPELLS, never over its type, so it reaches any
//! interchange-only shape a later clause admits. A block that spells ANYTHING -
//! a thematic break, or a paragraph holding a NO-BREAK SPACE (§7) - separates
//! the two lists as it always did and no boundary is written.
//!
//! THIS ENGINE ALREADY CONFORMS, by `writes_nothing` in `src/render_carve.rs`,
//! which is written over the rendered text rather than over the block kind and
//! is consulted by all three of the loops that track list adjacency: the
//! document level, `render_blocks`, and the tight-item writer
//! (markup-carve/carve-rs#1290). This file is the conformance pin for
//! markup-carve/carve-rs#1299, so the predicate cannot be narrowed back to a
//! type test without a named case going red.
//!
//! NO CARVE SOURCE REACHES THIS. A whitespace-only paragraph has no Carve
//! spelling, so the parse-driven corpus structurally cannot hold the tree and
//! the payloads below enter through the AST ingest. They deliberately do not
//! enter through the importer either: PART 11 §7 stops an import producing this
//! tree at all, so an import-driven fixture would quit reproducing.

fn list(item: &str) -> String {
    format!(
        r#"{{"type":"list","ordered":false,"tight":true,"bulletChar":"-","items":[{{"type":"list_item","children":[{{"type":"paragraph","children":[{{"type":"text","value":"{item}"}}]}}]}}]}}"#
    )
}

fn between(middle: &str) -> carve::Document {
    let children = if middle.is_empty() {
        format!("{},{}", list("a"), list("b"))
    } else {
        format!("{},{},{}", list("a"), middle, list("b"))
    };
    carve::from_json(&format!(
        r#"{{"type":"document","srcByteLength":0,"children":[{children}]}}"#
    ))
    .expect("ingest")
}

/// The source two genuinely adjacent sibling lists are written as, read from
/// the engine rather than spelled here: the rule is "as if nothing had stood
/// there", so the comparison has to be against whatever that is.
fn adjacent_source() -> String {
    carve::render_carve(&between("")).expect("write")
}

#[test]
fn two_sibling_lists_written_adjacent_do_not_merge_on_re_parse() {
    // The premise. If the boundary itself did not separate them, everything
    // below would be measuring the wrong thing.
    let source = adjacent_source();

    assert_eq!(
        carve::to_html(&source).matches("<ul>").count(),
        2,
        "{source:?}"
    );
}

#[test]
fn a_block_that_spells_nothing_leaves_the_lists_adjacent() {
    for middle in [
        // The shape §10j is written from.
        r#"{"type":"paragraph","children":[{"type":"text","value":" "}]}"#,
        // Tabs and newlines are the same class (PART 2's `whitespace` terminal
        // plus the terminators folded into it).
        r#"{"type":"paragraph","children":[{"type":"text","value":"\t\n  "}]}"#,
        // The sibling shape the clause names as already correct, kept here so a
        // change that reaches only one of the two cannot pass.
        r#"{"type":"paragraph","children":[]}"#,
    ] {
        let doc = between(middle);
        let source = carve::render_carve(&doc).expect("write");

        assert_eq!(source, adjacent_source(), "middle {middle}");
        assert_eq!(
            carve::to_html(&source).matches("<ul>").count(),
            2,
            "the lists merged: {source:?}"
        );
    }
}

#[test]
fn a_block_that_spells_something_still_separates_them() {
    // The control that separates §10j from "always write the boundary". Both of
    // these put characters on the page, so the lists are NOT adjacent and the
    // boundary must not appear.
    for middle in [
        r#"{"type":"thematic_break"}"#,
        // PART 11 §7: a NO-BREAK space is content, and a lone one parses back as
        // a paragraph - so this block really does put something in the source.
        "{\"type\":\"paragraph\",\"children\":[{\"type\":\"text\",\"value\":\"\u{a0}\"}]}",
    ] {
        let doc = between(middle);
        let source = carve::render_carve(&doc).expect("write");

        assert_ne!(source, adjacent_source(), "middle {middle}");
        assert_eq!(
            carve::to_html(&source),
            carve::render_html(&doc).expect("render"),
            "a block that spells its own content round trips whole: {source:?}"
        );
    }
}

#[test]
fn the_rule_reaches_the_nested_spellings_of_the_same_loop() {
    // The same list-separator decision is written three times in this engine -
    // the document level, `render_blocks` and the tight-item writer - and a fix
    // that catches two of them leaves the nested case broken
    // (markup-carve/carve-rs#1290). Inside an item is the spelling the document
    // level cannot reach.
    let inner = format!(
        r#"{{"type":"list_item","children":[{},{},{}]}}"#,
        list("a"),
        r#"{"type":"paragraph","children":[{"type":"text","value":" "}]}"#,
        list("b")
    );
    let outer = format!(
        r#"{{"type":"document","srcByteLength":0,"children":[{{"type":"list","ordered":false,"tight":true,"bulletChar":"*","items":[{inner}]}}]}}"#
    );
    let doc = carve::from_json(&outer).expect("ingest");
    let source = carve::render_carve(&doc).expect("write");

    assert_eq!(
        carve::to_html(&source).matches("<ul>").count(),
        3,
        "the two nested lists merged: {source:?}"
    );
}
