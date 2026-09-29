//! Comment payload keeps columns beyond its enclosing container prefix.

use carve::{to_carve, to_html};

#[test]
fn a_below_column_body_loses_only_the_available_host_prefix() {
    assert_eq!(
        to_carve("- a\n %%% n\n x\n %%%\n tail\n"),
        "- a\n  %%%\n  n\n  x\n  %%%\n  tail\n"
    );
}

#[test]
fn the_nested_form_agrees_too() {
    assert_eq!(
        to_carve("- - a\n %%% c\n x\n %%%\n b\n"),
        "- - a\n    %%%\n    c\n    x\n    %%%\n    b\n"
    );
}

#[test]
fn a_document_body_keeps_its_indentation() {
    assert_eq!(to_carve("%%%\n  x\n%%%\n"), "%%%\n  x\n%%%\n");
}

#[test]
fn a_body_shallower_than_the_host_keeps_its_text() {
    assert_eq!(
        to_carve("- a\n  %%%\n x\n  %%%\n"),
        "- a\n  %%%\n  x\n  %%%\n"
    );
}

#[test]
fn a_tab_straddling_the_host_column_keeps_its_residual_columns() {
    assert_eq!(
        to_carve("- a\n %%% n\n\tx\n %%%\n tail\n"),
        "- a\n  %%%\n  n\n    x\n  %%%\n  tail\n"
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
