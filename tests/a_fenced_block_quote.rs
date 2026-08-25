//! markup-carve/carve#1718. A colon fence whose type token is a bare `>` is a
//! second SPELLING of the block quote: the tree it produces is the one the
//! `>`-prefixed form produces, so every assertion here compares the two
//! spellings rather than pinning HTML.

use carve::ast::BlockNode;
use carve::{parse, to_carve, to_html};

#[test]
fn renders_the_element_the_prefixed_form_renders() {
    assert_eq!(to_html("::: >\nhello\n:::\n"), to_html("> hello\n"));
}

#[test]
fn nests_in_itself_at_constant_fence_width_leaving_nothing_behind() {
    let nested = "::: >\nouter\n\n::: >\ninner\n:::\n:::\n";
    assert_eq!(parse(nested).children.len(), 1);
    assert_eq!(to_html(nested), to_html("> outer\n>\n> > inner\n"));
}

#[test]
fn keeps_the_spelling_it_was_written_in() {
    // Asserting the node too: the fenced source round-trips byte for byte even
    // when it is not a quote at all, because an unrecognized fence is a
    // paragraph holding those same bytes. Without this the test passes before
    // the feature exists.
    assert!(matches!(
        parse("::: >\nhello\n:::\n").children.first(),
        Some(BlockNode::BlockQuote(_))
    ));
    assert_eq!(to_carve("::: >\nhello\n:::\n"), "::: >\nhello\n:::\n");
    assert_eq!(to_carve("> hello\n"), "> hello\n");
}
