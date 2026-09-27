//! `CARVE-P4-007`: a `class` key-value is a SPELLING OF THE CLASS SLOT. A parser
//! appends the value to `attrs.classes` in source order and writes nothing to
//! `attrs.keyValues`, so `{class=foo}` and `{.foo}` parse to one tree.
//!
//! Split across two slots it rendered TWO `class` attributes on one element,
//! which no HTML parser reads as the author wrote (carve#2439), and it forced
//! every consumer of a tree to read both slots or silently lose one (carve#2438).
//!
//! The two spellings are not interchangeable in SOURCE: `.` reads the
//! `explicit_identifier` a fence word does, while a value reaches past it, so
//! `-col` and `w-1/2` are classes only the key-value form can spell (carve#2435).
//! The writer therefore picks the spelling from the value rather than from a
//! remembered one, which is what keeps the fold round-trippable.

use carve::ast::{AttrSlot, BlockNode};

fn attrs_of(source: &str) -> carve::ast::Attrs {
    let doc = carve::parse(source);
    match &doc.children[0] {
        BlockNode::Directive(d) => d.attrs.clone(),
        BlockNode::Div(d) => d.attrs.clone(),
        BlockNode::Admonition(a) => a.attrs.clone(),
        other => panic!("unexpected block: {other:?}"),
    }
    .expect("the attribute line attached")
}

fn container(attr_line: &str) -> String {
    format!("{attr_line}\n::: x\ny\n:::\n")
}

/// How many `class=` attributes the element carries. Two is invalid HTML.
fn class_attributes(html: &str) -> usize {
    html.lines()
        .next()
        .unwrap_or_default()
        .matches("class=")
        .count()
}

#[test]
fn a_class_key_value_lands_in_the_class_slot() {
    let attrs = attrs_of(&container("{class=a .b}"));
    assert_eq!(attrs.classes, vec!["a", "b"]);
    assert!(
        attrs.key_values.is_empty(),
        "the class must not also reach key_values: {:?}",
        attrs.key_values
    );
    // ONE slot entry for the merged attribute, the way the `.` shorthand records
    // it - a second would have the writer spell an attribute that does not exist.
    assert_eq!(attrs.order, vec![AttrSlot::Class]);
}

#[test]
fn the_class_slot_keeps_source_order_across_both_spellings() {
    assert_eq!(attrs_of(&container("{class=a .b}")).classes, vec!["a", "b"]);
    assert_eq!(attrs_of(&container("{.b class=a}")).classes, vec!["b", "a"]);
    assert_eq!(
        attrs_of(&container("{class=a class=b}")).classes,
        vec!["a", "b"],
        "a second key-value appends rather than overwriting"
    );
}

#[test]
fn no_shape_renders_two_class_attributes() {
    // The defect the ticket names, plus every neighbouring spelling that reaches
    // the same slot. A bare boolean `class` is here because PART 4 gives it the
    // same empty-string value as `class=""`, so the two must build one tree.
    for attr_line in [
        "{class=a .b}",
        "{.b class=a}",
        "{class=-col}",
        "{class=a class=b}",
        "{class}",
        "{class=\"\"}",
        "{.a .b}",
        "{#i class=-col .b k=v}",
    ] {
        let html = carve::to_html(&container(attr_line));
        assert_eq!(class_attributes(&html), 1, "{attr_line} rendered {html}");
    }
    // A span takes the same slot, so the inline path answers alike.
    assert_eq!(class_attributes(&carve::to_html("[t]{class=a .b}\n")), 1);
}

#[test]
fn a_class_the_shorthand_cannot_spell_is_written_as_a_quoted_key_value() {
    // Written `.` these were source this engine's own parser reads as a
    // paragraph, so the fold would have made the writer emit invalid Carve.
    // Quoted, because `unquoted_value` cannot hold `w-1/2` either (carve#2440),
    // and because `{class="-col"}` is the spelling carve#2435 ruled.
    for (attr_line, expected) in [
        ("{class=-col}", "{class=\"-col\"}"),
        ("{class=\"w-1/2\" .grid}", "{class=\"w-1/2\" .grid}"),
        ("{class}", "{class=\"\"}"),
        ("{class=a .b}", "{.a .b}"),
        ("{#i class=-col .b k=v}", "{#i class=\"-col\" .b k=v}"),
    ] {
        let written = carve::to_carve(&container(attr_line));
        assert_eq!(
            written.lines().next().unwrap_or_default(),
            expected,
            "{attr_line}"
        );
    }
}

#[test]
fn every_spelling_round_trips_through_the_writer() {
    // The invariant the fold could break: what the writer emits has to render
    // the same document, which it cannot do if its own parser refuses the bytes.
    for attr_line in [
        "{class=a .b}",
        "{.b class=a}",
        "{class=-col}",
        "{class=\"w-1/2\" .grid}",
        "{class}",
        "{class=\"\"}",
        "{class=\"a|b\"}",
        "{#i class=-col .b k=v}",
        "{.a .b}",
    ] {
        let source = container(attr_line);
        let once = carve::to_html(&source);
        let written = carve::to_carve(&source);
        assert_eq!(carve::to_html(&written), once, "{attr_line} -> {written:?}");
        // And idempotent, so the second pass is not a different document again.
        assert_eq!(carve::to_carve(&written), written, "{attr_line}");
    }
}
