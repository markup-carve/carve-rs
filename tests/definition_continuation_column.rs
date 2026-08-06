//! A definition body's continuation indent is a COLUMN claim, not a character
//! count -- the definition-list twin of `footnote_continuation_column`.
//!
//! PART 9 §24 C1 gives a tab a column value: it advances to the next multiple
//! of 4 from wherever it starts. A definition body's continuation is a leading
//! INDENTATION run, so a tab is syntax there and the floor of three is a floor
//! of three COLUMNS (spec carve#692, written into the grammar by carve#796 for
//! the footnote twin, and asked for the definition twin by carve#893).
//!
//! This engine already reads it that way, in both of the two places that spell
//! the rule -- `collect_definition_body`'s blank-line lookahead, which decides
//! whether the body CONTINUES, and its form-A branch, which decides how far the
//! line DEDENTS. Neither was pinned: every mutation named below leaves the rest
//! of the suite green, so a later sweep "aligning" this engine with the three
//! readers that still spell the run in literal spaces would land silently.
//!
//! The three non-conforming spellings, each of which some mutation below
//! reproduces exactly:
//!
//! ```text
//!   oracle     3+ literal spaces followed by a non-space
//!   carve-php  3+ literal spaces
//!   carve-js   a whitespace run of 3+ CHARACTERS (a tab counting as one)
//! ```
//!
//! A refused continuation does not merely indent differently: the line LEAVES
//! the `<dd>` and becomes a document-level block after `</dl>`, so the content
//! moves out of the definition entirely. Each case asserts where the text
//! landed, not just that it survived.

use carve::{parse, render_html};

fn html(src: &str) -> String {
    render_html(&parse(src)).expect("render")
}

/// Does a line indented by `indent`, after a blank line, stay in the `<dd>`?
///
/// The body's own column is three, so this asks the rule directly. "Inside"
/// means the text appears before the list closes; a refused continuation
/// renders after `</dl>`.
fn continues(indent: &str) -> bool {
    let out = html(&format!(":: t\n:  d\n\n{indent}more\n"));
    let text = out.find("more").expect("the text is never dropped");
    let close = out.find("</dl>").expect("the list always closes");
    text < close
}

// --- the floor is three COLUMNS ------------------------------------------

#[test]
fn three_literal_spaces_continue_the_body() {
    // Column 3 exactly: the floor is reached, not passed.
    assert!(continues("   "));
}

#[test]
fn two_spaces_do_not_continue_the_body() {
    assert!(!continues("  "), "column 2 is below the floor");
}

#[test]
fn one_space_does_not_continue_the_body() {
    assert!(!continues(" "), "column 1 is below the floor");
}

// --- a tab is syntax in the run, and carries its column value ------------

#[test]
fn a_bare_tab_continues_the_body() {
    assert!(continues("\t"), "a tab from column 0 reaches column 4");
}

#[test]
fn a_space_then_a_tab_continues_the_body() {
    // Mixed run, space first: the tab advances from column 1 to column 4.
    assert!(continues(" \t"));
}

#[test]
fn a_tab_then_a_space_continues_the_body() {
    // Mixed run, tab first: column 4, then 5. The same rule read from the
    // other end -- a run rule written as a first-character test passes one of
    // these two and fails the other.
    assert!(continues("\t "));
}

#[test]
fn two_spaces_then_a_tab_continue_the_body() {
    // Three whitespace CHARACTERS and column 4. A reader counting characters
    // takes this one while refusing a bare tab, which is why counting
    // characters is not the same rule wearing different clothes.
    assert!(continues("  \t"));
}

#[test]
fn three_spaces_then_a_tab_continue_the_body() {
    // The floor is already reached by the spaces; the tab past it is part of
    // the run, not the content. A reader demanding a non-space immediately
    // after three spaces refuses this one alone.
    assert!(continues("   \t"));
}

#[test]
fn two_tabs_continue_the_body() {
    assert!(continues("\t\t"), "columns 4 then 8");
}

// --- the dedent is measured in the same unit as the test ------------------

#[test]
fn the_dedent_is_by_column_not_by_character_count() {
    // `\t- a` reaches column 4 and `\t  - b` column 6, so b is a's child. The
    // body strips three COLUMNS off each. Stripping three CHARACTERS instead
    // takes the whole run off the first line and only part of it off the
    // second, landing both markers at the same column and flattening the
    // nesting into two sibling lists -- a corruption the CONTINUE/END check
    // above cannot see, because both lines are collected either way.
    let out = html(":: t\n:  d\n\n\t- a\n\t  - b\n");
    // Nesting is asserted directly rather than inferred from the order of the
    // outer list's own tags: b must open a SECOND `<ul>`, and that list must
    // open before any item closes. Reading `rfind("<ul>")` instead would also
    // accept a flattening to one list with two sibling items, since the sole
    // `<ul>` necessarily precedes every `</li>`.
    let inner_open = out
        .match_indices("<ul>")
        .nth(1)
        .unwrap_or_else(|| panic!("b did not open a sub-list at all: {out}"))
        .0;
    let first_close = out.find("</li>").expect("an item always closes");
    assert!(
        first_close > inner_open,
        "the sub-list was flattened into a sibling: {out}"
    );
}

// --- CONTROL --------------------------------------------------------------

#[test]
fn a_tab_with_no_blank_line_before_it_folds_whatever_the_rule_says() {
    // CONTROL, not evidence. With no blank line the text folds through lazy
    // continuation, which does not look at indentation at all -- a flush-left
    // line folds identically. No mutation of the column rule changes this
    // case, which is exactly why the probe in carve#878 §5 (this shape, with
    // no blank) reported agreement that was not there. It is kept so a future
    // reader can see the difference between the two shapes.
    let out = html(":: t\n:  d\n\tmore\n");
    assert!(out.find("more").unwrap() < out.find("</dl>").unwrap());
    let flush = html(":: t\n:  d\nmore\n");
    assert!(flush.find("more").unwrap() < flush.find("</dl>").unwrap());
}
