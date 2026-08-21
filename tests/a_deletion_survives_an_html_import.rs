//! carve-rs#1223: `<del>` imports as a strike emphasis, so it comes back as
//! `<s>`.
//!
//! The comment on the `<ins>` arm of the importer's inline match makes the
//! argument for `<del>` verbatim: the deletion marker `{- -}` has a node of its
//! own, `CriticDelete`, and it renders back to `<del>`. But `<del>` sat in the
//! strike arm with `<s>` and `<strike>`, so `a {-x-} b` rendered to
//! `<del>` and imported back as `~x~`, which re-renders as `<s>`. The element
//! changed: unlike the inherent shifts found in the same sweep
//! (markup-carve/carve-rs#1208), this one is not HTML-lossless. `<ins>`
//! round-trips exactly; the same CriticMarkup pair had two answers.
//!
//! It is a cross-engine import contract rather than a local call, and the other
//! two engines already agree: carve-js maps `del` to its `delete` node and
//! carve-php spells it `{- -}`.
//!
//! `<s>` and `<strike>` genuinely ARE strike and stay in that arm - `~x~` is
//! what Carve spells them with. The question was only whether `<del>`, which
//! has an exact node, should keep sharing it.

use carve::{
    html_to_ast, html_to_carve, parse, render_html, BlockNode, HtmlImportOptions, InlineNode,
};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

fn only_inline(html: &str) -> InlineNode {
    let doc = html_to_ast(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    match doc.children.into_iter().next() {
        Some(BlockNode::Paragraph(p)) => p
            .children
            .into_iter()
            .find(|node| !matches!(node, InlineNode::Text(_)))
            .expect("an element child"),
        other => panic!("expected a paragraph, got {other:?}"),
    }
}

#[test]
fn a_deletion_keeps_its_element() {
    let result =
        html_to_carve("<p>a <del>gone</del> b</p>", &HtmlImportOptions::default()).unwrap();
    assert_eq!(result.value, "a {-gone-} b\n");
    assert!(
        result.report.diagnostics.is_empty(),
        "an element Carve can spell is not a loss: {:?}",
        result.report.diagnostics
    );
    assert_eq!(
        render_html(&parse(&result.value)).unwrap(),
        "<p>a <del>gone</del> b</p>"
    );
}

/// The measurement from the ticket, both directions: the engine's own output
/// for `{- -}` comes back as what wrote it.
#[test]
fn the_engines_own_deletion_html_round_trips() {
    let html = render_html(&parse("a {-x-} b\n")).unwrap();
    assert_eq!(html, "<p>a <del>x</del> b</p>");
    assert_eq!(imported(&html), "a {-x-} b\n");
}

/// The NODE is what changed, and bytes cannot show it: a `delete` and a strike
/// emphasis are two different nodes, and every non-HTML writer has a case for
/// each.
#[test]
fn a_deletion_imports_as_the_deletion_node() {
    assert!(
        matches!(
            only_inline("<p>a <del>x</del></p>"),
            InlineNode::CriticDelete(_)
        ),
        "expected a CriticDelete"
    );
}

/// CONTROL - `<s>` genuinely IS strike, and `~x~` is what Carve spells it with.
#[test]
fn an_s_element_is_still_a_strike_emphasis() {
    assert_eq!(imported("<p>a <s>gone</s> b</p>"), "a ~gone~ b\n");
    assert!(
        matches!(only_inline("<p><s>x</s></p>"), InlineNode::Emphasis(_)),
        "expected an Emphasis"
    );
}

/// CONTROL - `<strike>`, the legacy spelling, travels with `<s>`.
#[test]
fn a_strike_element_is_still_a_strike_emphasis() {
    assert_eq!(imported("<p>a <strike>gone</strike> b</p>"), "a ~gone~ b\n");
}

/// CONTROL - a strike written in Carve still renders `<s>` and comes back as
/// itself. The two spellings stay distinguishable in both directions, which is
/// the whole point of giving `<del>` its own arm.
#[test]
fn a_strike_written_in_carve_still_round_trips_as_a_strike() {
    let html = render_html(&parse("a ~x~ b\n")).unwrap();
    assert_eq!(html, "<p>a <s>x</s> b</p>");
    assert_eq!(imported(&html), "a ~x~ b\n");
}

/// CONTROL - `<ins>`, the twin that already had its own node, is untouched.
#[test]
fn an_insertion_still_keeps_its_element() {
    assert_eq!(imported("<p>a <ins>added</ins> b</p>"), "a {+added+} b\n");
}

#[test]
fn a_deletion_carries_its_attributes() {
    assert_eq!(
        imported("<p><del class=\"cut\" data-who=\"me\">x</del></p>"),
        "{-x-}{.cut data-who=me}\n"
    );
}

/// A deletion nests like any other inline, and the pair survives the nesting.
#[test]
fn a_deletion_around_other_inlines_keeps_them() {
    assert_eq!(
        imported("<p><del>a <em>b</em> c</del></p>"),
        "{-a /b/ c-}\n"
    );
}
