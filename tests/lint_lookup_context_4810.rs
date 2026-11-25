//! carve-rs#4810: a lookup key that reads the record being tested is indexed per tag, with the match
//! groups of that record (C2-W-141), so a later blank line does not hide an unclosed opener.

use carve::lint::{LintOptions, lint};

fn openers(source: &str) -> Vec<(u32, u32)> {
    lint(source, &LintOptions::default())
        .iter()
        .filter(|d| d.code == "unclosed-inline-emphasis")
        .filter_map(|d| d.pos.map(|pos| (pos.start_line, pos.start_column)))
        .collect()
}

#[test]
fn footnote_reference_before_its_definition() {
    assert_eq!(openers("See [^n](/u).\n\n[^n]: Note.\n"), [(1, 10)]);
}

#[test]
fn paragraph_at_offset_zero_before_a_blank_line() {
    assert_eq!(openers("See /u.\n\n[^n]: Note.\n"), [(1, 5)]);
    assert_eq!(openers("See [^n] (/u).\n\n[^n]: Note.\n"), [(1, 11)]);
}

#[test]
fn bare_openers_after_a_paren_or_bracket() {
    assert_eq!(openers("See (/u).\n"), [(1, 6)]);
    assert_eq!(openers("x [*u\n"), [(1, 4)]);
}

#[test]
fn each_delimiter_gets_its_own_index() {
    assert_eq!(
        openers("See *u and /v.\n\n[^n]: Note.\n"),
        [(1, 5), (1, 12)]
    );
    assert!(openers("See [a](/u).\n\n[^n]: Note.\n").is_empty());
    assert_eq!(openers("See /u\nand /z.\n\n[^n]: Note.\n"), [(2, 5)]);
}
