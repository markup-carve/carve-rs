//! A `%%` run on Markdown import took an escape on EVERY percent sign, so
//! `plain %%c here` imported as `plain \%\%c here` where carve-js and carve-php
//! both write `plain \%%c here`.
//!
//! A comment opens on the first two UNESCAPED percent signs, so the escape on
//! the first one already makes the whole run literal - both spellings render
//! the same text, and the shared escaper corpus
//! (`tests/corpus-escape/cases.json`) pins the one-escape form
//! (carve-rs#2443).

use carve::{markdown_to_carve, to_html, to_plain_text};

#[test]
fn one_escape_carries_a_percent_run() {
    assert_eq!(
        markdown_to_carve("plain %%c here\n"),
        "plain \\%%c here\n",
        "a percent run takes one escape"
    );
    assert_eq!(markdown_to_carve("%%c at start\n"), "\\%%c at start\n");
    assert_eq!(markdown_to_carve("a %%c%% b\n"), "a \\%%c%% b\n");
    assert_eq!(markdown_to_carve("a %% b\n"), "a \\%% b\n");
}

/// A longer run opens on its first two percent signs as well, so one escape is
/// the whole of what it owes.
#[test]
fn a_longer_percent_run_takes_one_escape() {
    assert_eq!(markdown_to_carve("a %%%c b\n"), "a \\%%%c b\n");
    assert_eq!(markdown_to_carve("a %%%%c b\n"), "a \\%%%%c b\n");
}

/// Both spellings read back as the same text, which is why the shorter one is
/// the right one; the bare run is a comment and loses the rest of the line.
#[test]
fn the_shorter_spelling_reads_the_same_text() {
    for (one, every) in [
        ("plain \\%%c here\n", "plain \\%\\%c here\n"),
        ("a \\%%%c b\n", "a \\%\\%\\%c b\n"),
    ] {
        assert_eq!(to_plain_text(one), to_plain_text(every));
        assert_eq!(to_html(one), to_html(every));
    }
    assert_eq!(to_plain_text("plain \\%%c here\n"), "plain %%c here\n");
    assert!(!to_plain_text("plain %%c here\n").contains("%%"));
}

/// A percent that needs no escape never gains one.
#[test]
fn a_percent_that_opens_nothing_stays_bare() {
    assert_eq!(markdown_to_carve("a % b\n"), "a % b\n");
    assert_eq!(markdown_to_carve("100%%x\n"), "100%%x\n");
}

/// A run the Markdown source already escaped does not gain a second escape.
#[test]
fn an_already_escaped_run_keeps_its_one_escape() {
    assert_eq!(
        markdown_to_carve("plain \\%%c here\n"),
        "plain \\%%c here\n"
    );
}

/// The control: a real comment in authored Carve still parses as a comment.
#[test]
fn an_authored_comment_still_parses() {
    assert_eq!(
        to_plain_text("before\n%% a comment\nafter\n"),
        "before\n\nafter\n"
    );
    assert_eq!(to_plain_text("after %% trailing\n"), "after\n");
}
