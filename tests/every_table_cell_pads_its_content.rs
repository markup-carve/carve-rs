//! PART 11: the canonical form of a table cell is its prefix glued to the
//! opening pipe, then ONE space, then the content, then one space before the
//! closing pipe.
//!
//! The prefix has to touch the pipe - a space in front of `=` or of an
//! attribute block makes it literal content - but the content does not, and
//! the padded form is both the readable one and the safe one. The alignment
//! sigil and the attribute slot are read GLUED off the untrimmed cell, so a
//! glued content character was handed to one of them: `| ~x~ |` came back as
//! `|=~x~|`, a CENTERED column holding `x~` (carve-rs#819). That used to be two
//! guards enumerating the characters that merge; the space covers every cell.

use carve::{to_carve, to_html};

fn fmt(src: &str) -> String {
    to_carve(src)
}

#[test]
fn a_header_cell_pads_its_content() {
    assert_eq!(fmt("| Heading |\n|---|\n| a |\n"), "|= Heading |\n| a |\n");
}

#[test]
fn an_attributed_header_cell_pads_its_content_too() {
    assert_eq!(
        fmt("|{.total} Total | 99 |\n|---|---|\n| a | b |\n"),
        "|={.total} Total |= 99 |\n| a | b |\n"
    );
}

#[test]
fn an_alignment_marker_stays_in_the_prefix() {
    assert_eq!(
        fmt("|= Item |=> Score |\n| Pen |>{.num} 9 |\n"),
        "|= Item |=> Score |\n| Pen |>{.num} 9 |\n"
    );
}

#[test]
fn an_empty_cell_takes_a_single_space() {
    // Two would grow the column by one space per format run.
    assert_eq!(fmt("| |x |\n|---|\n| y |\n"), "|= |= x |\n| y |\n");
    assert_eq!(fmt("| h |\n|---|\n| |x |\n"), "|= h |\n| | x |\n");
}

#[test]
fn a_content_sigil_stays_content() {
    // The space after the prefix is what parts them: glued, the alignment scan
    // would eat the `~` and center the column.
    let src = "| ~x~ |\n|---|\n| y |\n";
    let out = fmt(src);
    assert_eq!(out, "|= ~x~ |\n| y |\n");
    assert_eq!(to_html(&out), to_html(src));
}

#[test]
fn a_content_brace_stays_content() {
    let src = "|= a |\n| {#i} b |\n";
    let out = fmt(src);
    assert_eq!(to_html(&out), to_html(src));
}

#[test]
fn formatting_is_idempotent() {
    for src in [
        "|{.total} Total | 99 |\n|---|---|\n| a | b |\n",
        "| |x |\n|---|\n| y |\n",
        "|=<{.h} Name |=>{.c} Score |{.head}\n| Ann |>{.num} 9 |{.win}\n",
    ] {
        let once = fmt(src);
        assert_eq!(fmt(&once), once, "not idempotent for {src:?}");
        assert_eq!(to_html(&once), to_html(src), "round trip lost for {src:?}");
    }
}
