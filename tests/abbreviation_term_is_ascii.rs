//! An abbreviation term is ASCII, because `letter` is.
//!
//! The grammar spells the term out:
//!
//! ```text
//! abbreviation_term = (letter | digit)+ ;
//! letter = 'a' | ... | 'z' | 'A' | ... | 'Z' ;
//! digit  = '0' | ... | '9' ;
//! ```
//!
//! `letter` is an enumerated ASCII set, so `*[ß]: sharp s` is not a definition -
//! it is paragraph text, for the same reason `*[e.g.]:` is, which all three
//! engines already agree on.
//!
//! This engine used `char::is_alphanumeric`, which is Unicode-aware, so a
//! non-ASCII term WAS a definition here and paragraph text in carve-js and
//! carve-php (carve#791). An abbreviation definition renders nothing and
//! silently changes the text around it, so the disagreement costs the reader
//! either the line or the expansion.
//!
//! `is_attr_ident_start` two functions below already carries this reasoning for
//! attribute identifiers: "Non-ASCII bytes are never a start here".

use carve::{parse, render_html};

fn html(src: &str) -> String {
    render_html(&parse(src)).expect("render")
}

fn defines(label: &str) -> bool {
    let src = format!("*[{label}]: expansion\n\nuse {label} here.\n");
    html(&src).contains("<abbr")
}

#[test]
fn an_ascii_term_is_a_definition() {
    for label in ["HTML", "D", "d", "ab", "aB", "1a", "9", "x9y"] {
        assert!(defines(label), "`*[{label}]:` should be a definition");
    }
}

#[test]
fn a_non_ascii_term_is_paragraph_text() {
    // Letters in their own scripts, all outside the enumerated `letter` set.
    for label in ["ß", "Å", "é", "日本", "Ω"] {
        assert!(
            !defines(label),
            "`*[{label}]:` should be paragraph text - `letter` is ASCII"
        );
    }
}

#[test]
fn the_line_itself_survives_when_it_is_not_a_definition() {
    // The failure mode this guards: declining the term must leave the line as
    // text, not consume it. A vanished line is the defect the fuzzer found in
    // this engine before (carve-rs#451).
    let out = html("*[ß]: sharp s\n");
    assert!(out.contains("*[ß]: sharp s"), "line vanished: {out}");
}

#[test]
fn punctuation_is_still_not_a_term() {
    // Unchanged behaviour, pinned so the ASCII narrowing cannot widen anything.
    for label in ["e.g.", "HTTP API", "x-y"] {
        assert!(!defines(label), "`*[{label}]:` should be paragraph text");
    }
}
