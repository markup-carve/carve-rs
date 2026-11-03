//! Comment payload keeps columns beyond its enclosing container prefix.

use carve::{to_carve, to_html};

// Under rule A a span opened below the item's column is a document-level block.
#[test]
fn a_below_column_body_loses_only_the_available_host_prefix() {
    assert_eq!(
        to_carve("- a\n %%% n\n x\n %%%\n tail\n"),
        "- a\n\n%%%\nn\n x\n%%%\n\ntail\n"
    );
}

#[test]
fn the_nested_form_agrees_too() {
    assert_eq!(
        to_carve("- - a\n %%% c\n x\n %%%\n b\n"),
        "- - a\n\n%%%\nc\n x\n%%%\n\nb\n"
    );
}

#[test]
fn a_document_body_keeps_its_indentation() {
    assert_eq!(to_carve("%%%\n  x\n%%%\n"), "%%%\n  x\n%%%\n");
}

#[test]
fn a_matched_comment_body_keeps_its_complete_extent() {
    crate::common::assert_rule_a_pins(
        "comment_body_is_relative_to_its_host::a_matched_comment_body_keeps_its_complete_extent",
    );
}

#[test]
fn a_tab_straddling_the_host_column_keeps_its_residual_columns() {
    assert_eq!(
        to_carve("- a\n %%% n\n\tx\n %%%\n tail\n"),
        "- a\n\n%%%\nn\n\tx\n%%%\n\ntail\n"
    );
}

#[test]
fn the_content_the_body_carries_is_unchanged() {
    assert_eq!(
        to_html("- a\n %%% n\n x\n %%%\n tail\n"),
        to_html(&to_carve("- a\n %%% n\n x\n %%%\n tail\n"))
    );
    assert!(!to_html("- a\n %%% n\n x\n %%%\n tail\n").contains('x'));
}

#[test]
fn fmt_still_settles_on_the_first_pass() {
    for src in [
        "- a\n %%% n\n x\n %%%\n tail\n",
        "- - a\n %%% c\n x\n %%%\n b\n",
        "%%%\n  x\n%%%\n",
    ] {
        let once = to_carve(src);
        assert_eq!(to_carve(&once), once, "second pass differs for {src:?}");
    }
}
